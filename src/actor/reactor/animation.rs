use std::time::{Duration, Instant};

use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use tracing::{debug, trace};

use super::TransactionId;
use crate::actor::app::{AppThreadHandle, Request, WindowId, pid_t};
use crate::actor::reactor::Reactor;
use crate::common::collections::HashMap;
use crate::common::config::{AnimationEasing, LayoutMode};
use crate::sys::geometry::{Round, SameAs};
use crate::sys::power;
use crate::sys::screen::SpaceId;
use crate::sys::window_server::WindowServerId;

#[derive(Debug)]
pub struct Animation<'a> {
    configured_fps: f64,
    duration: Duration,
    windows: Vec<(
        &'a AppThreadHandle,
        WindowId,
        CGRect,
        CGRect,
        bool,
        TransactionId,
    )>,
}

impl<'a> Animation<'a> {
    pub fn new(fps: f64, duration: f64, _: AnimationEasing) -> Self {
        Animation {
            configured_fps: fps.min(60.0),
            duration: Duration::from_secs_f64(duration.max(0.0)),
            windows: vec![],
        }
    }

    pub fn add_window(
        &mut self,
        handle: &'a AppThreadHandle,
        wid: WindowId,
        start: CGRect,
        finish: CGRect,
        is_focus: bool,
        txid: TransactionId,
    ) {
        self.windows.push((handle, wid, start, finish, is_focus, txid))
    }

    pub fn run(self) {
        if self.windows.is_empty() {
            return;
        }

        for &(handle, wid, from, to, is_focus, txid) in &self.windows {
            _ = handle.send(Request::BeginWindowAnimation(wid));
            // Resize new windows immediately, but keep pure translations on
            // the cheaper position-only path for the full animation.
            if is_focus && size_changed(from, to) {
                let frame = CGRect {
                    origin: from.origin,
                    size: to.size,
                };
                _ = handle.send(Request::SetWindowFrame(wid, frame, txid, false));
            }
        }

        let start = Instant::now();
        let duration_secs = self.duration.as_secs_f64();
        let mut next_frames = Vec::with_capacity(self.windows.len());
        let mut sent_mid_resize = false;

        loop {
            let elapsed = start.elapsed();
            if elapsed < self.duration {
                let fps = self.effective_fps();
                let interval = Duration::from_secs_f64(1.0 / fps);
                std::thread::sleep(interval.min(self.duration - elapsed));
            }

            let t = if duration_secs == 0.0 {
                1.0
            } else {
                (start.elapsed().as_secs_f64() / duration_secs).min(1.0)
            };
            let is_final = t >= 1.0;
            let should_resize = is_final || (!sent_mid_resize && t >= 0.5);

            next_frames.clear();
            for (_, _, from, to, _, _) in &self.windows {
                next_frames.push(get_frame(*from, *to, t));
            }

            let mut frames_by_pid: HashMap<pid_t, (&AppThreadHandle, TransactionId, Vec<(WindowId, CGRect)>)> =
                HashMap::default();
            let mut positions_by_pid: HashMap<
                pid_t,
                (&AppThreadHandle, TransactionId, Vec<(WindowId, CGPoint)>),
            > = HashMap::default();

            for (&(handle, wid, from, to, _, txid), rect) in self.windows.iter().zip(&next_frames)
            {
                let mut rect = *rect;
                // Round interpolated positions to whole pixels to prevent a
                // feedback loop: sub-pixel values get rounded by macOS, rift
                // sees the rounded frame as a change, recalculates layout with
                // a slightly different result, and the window drifts.
                rect.origin = rect.origin.round();
                // Actually don't animate size, too slow. Resize halfway through
                // and then set the size again at the end, in case it got
                // clipped during the animation.
                if should_resize && size_changed(from, to) {
                    rect.size = to.size;
                    let entry = frames_by_pid
                        .entry(wid.pid)
                        .or_insert((handle, txid, Vec::new()));
                    entry.2.push((wid, rect));
                } else {
                    let entry = positions_by_pid
                        .entry(wid.pid)
                        .or_insert((handle, txid, Vec::new()));
                    entry.2.push((wid, rect.origin));
                }
            }

            for (_, (handle, txid, frames)) in frames_by_pid {
                if frames.len() == 1 {
                    let (wid, rect) = frames.into_iter().next().unwrap();
                    _ = handle.send(Request::SetWindowFrame(wid, rect, txid, false));
                } else {
                    _ = handle.send(Request::SetBatchWindowFrame(frames, txid));
                }
            }
            for (_, (handle, txid, positions)) in positions_by_pid {
                if positions.len() == 1 {
                    let (wid, pos) = positions.into_iter().next().unwrap();
                    _ = handle.send(Request::SetWindowPos(wid, pos, txid, false));
                } else {
                    _ = handle.send(Request::SetBatchWindowPos(positions, txid));
                }
            }

            if should_resize && !is_final {
                sent_mid_resize = true;
            }
            if is_final {
                break;
            }
        }

        for &(handle, wid, ..) in &self.windows {
            _ = handle.send(Request::EndWindowAnimation(wid));
        }
    }

    fn effective_fps(&self) -> f64 {
        self.windows
            .iter()
            .map(|(handle, ..)| handle.adaptive_animation_fps(self.configured_fps))
            .fold(self.configured_fps, f64::min)
            .max(1.0)
    }

    #[allow(dead_code)]
    pub fn skip_to_end(self) {
        for &(handle, wid, _from, to, _, txid) in &self.windows {
            _ = handle.send(Request::SetWindowFrame(wid, to, txid, true));
        }
    }
}

fn get_frame(a: CGRect, b: CGRect, t: f64) -> CGRect {
    let s = ease(t);
    CGRect {
        origin: CGPoint {
            x: blend(a.origin.x, b.origin.x, s),
            y: blend(a.origin.y, b.origin.y, s),
        },
        size: CGSize {
            width: blend(a.size.width, b.size.width, s),
            height: blend(a.size.height, b.size.height, s),
        },
    }
}

// https://notes.yvt.jp/Graphics/Easing-Functions/
fn ease(t: f64) -> f64 {
    if t < 0.5 {
        (1.0 - f64::sqrt(1.0 - f64::powi(2.0 * t, 2))) / 2.0
    } else {
        (f64::sqrt(1.0 - f64::powi(-2.0 * t + 2.0, 2)) + 1.0) / 2.0
    }
}

fn blend(a: f64, b: f64, s: f64) -> f64 {
    (1.0 - s) * a + s * b
}

fn size_changed(current: CGRect, target: CGRect) -> bool {
    !current.size.same_as(target.size)
}

fn visible_width_ratio(frame: CGRect, screen: CGRect) -> f64 {
    let visible_left = frame.origin.x.max(screen.origin.x);
    let visible_right = frame.max().x.min(screen.max().x);
    let visible_width = (visible_right - visible_left).max(0.0);
    if frame.size.width <= 0.0 {
        0.0
    } else {
        visible_width / frame.size.width
    }
}

fn should_skip_scrolling_animation(current: CGRect, target: CGRect, screen: CGRect) -> bool {
    const MIN_VISIBLE_RATIO_FOR_ANIMATION: f64 = 0.1;

    visible_width_ratio(current, screen) < MIN_VISIBLE_RATIO_FOR_ANIMATION
        && visible_width_ratio(target, screen) < MIN_VISIBLE_RATIO_FOR_ANIMATION
}

pub struct AnimationManager;

impl AnimationManager {
    pub fn animate_layout(
        reactor: &mut Reactor,
        space: SpaceId,
        layout: &[(WindowId, CGRect)],
        is_resize: bool,
        skip_wid: Option<WindowId>,
    ) -> bool {
        let Some(active_ws) = reactor.layout_manager.layout_engine.active_workspace(space) else {
            return false;
        };
        let mut anim = Animation::new(
            reactor.config.settings.animation_fps,
            reactor.config.settings.animation_duration,
            reactor.config.settings.animation_easing.clone(),
        );
        let scrolling_screen = (reactor.layout_manager.layout_engine.active_layout_mode_at(space)
            == LayoutMode::Scrolling)
            .then(|| reactor.space_manager.screen_by_space(space).map(|screen| screen.frame))
            .flatten();
        let mut animated_count = 0;
        let mut animated_wids_wsids: Vec<u32> = Vec::new();
        let mut any_frame_changed = false;

        for &(wid, target_frame) in layout {
            // Skip applying layout frames and animations for the window currently being dragged.
            if skip_wid == Some(wid) {
                trace!(
                    ?wid,
                    "Skipping animated layout update for window currently being dragged"
                );
                continue;
            }

            let target_frame = target_frame.round();
            let (current_frame, window_server_id, txid) =
                match reactor.window_manager.windows.get_mut(&wid) {
                    Some(window) => {
                        let current_frame = window.frame_monotonic;
                        if target_frame.same_as(current_frame) {
                            continue;
                        }
                        let wsid = window.info.sys_id.unwrap();
                        if reactor
                            .transaction_manager
                            .get_target_frame(wsid)
                            .is_some_and(|pending| pending.same_as(target_frame))
                        {
                            trace!(?wid, ?target_frame, "Skipping redundant layout request");
                            continue;
                        }
                        any_frame_changed = true;
                        let txid = reactor.transaction_manager.generate_next_txid(wsid);
                        (current_frame, Some(wsid), txid)
                    }
                    None => {
                        debug!(?wid, "Skipping - window no longer exists");
                        continue;
                    }
                };

            let Some(app_state) = &reactor.app_manager.apps.get(&wid.pid) else {
                debug!(?wid, "Skipping for window - app no longer exists");
                continue;
            };

            let is_active = reactor
                .layout_manager
                .layout_engine
                .virtual_workspace_manager()
                .workspace_for_window(space, wid)
                .map_or(false, |ws| ws == active_ws);
            let skip_edge_parked_animation = scrolling_screen.is_some_and(|screen| {
                should_skip_scrolling_animation(current_frame, target_frame, screen)
            });
            if let Some(screen) = scrolling_screen {
                let current_visible = visible_width_ratio(current_frame, screen);
                let target_visible = visible_width_ratio(target_frame, screen);
                if current_visible < 1.0 || target_visible < 1.0 {
                    debug!(
                        ?wid,
                        ?current_frame,
                        ?target_frame,
                        is_active,
                        current_visible,
                        target_visible,
                        skip_edge_parked_animation,
                        "Scrolling animation decision"
                    );
                }
            }

            if is_active && !skip_edge_parked_animation {
                trace!(?wid, ?current_frame, ?target_frame, "Animating visible window");
                animated_wids_wsids.push(wid.idx.into());
                anim.add_window(&app_state.handle, wid, current_frame, target_frame, false, txid);
                animated_count += 1;
                if let Some(wsid) = window_server_id {
                    reactor.transaction_manager.update_txid_entries([(wsid, txid, target_frame)]);
                }
            } else {
                trace!(
                    ?wid,
                    ?current_frame,
                    ?target_frame,
                    skip_edge_parked_animation,
                    "Direct positioning non-animated window"
                );
                if let Some(wsid) = window_server_id {
                    reactor.transaction_manager.update_txid_entries([(wsid, txid, target_frame)]);
                }
                if let Err(e) =
                    app_state.handle.send(Request::SetWindowFrame(wid, target_frame, txid, true))
                {
                    debug!(?wid, ?e, "Failed to send frame request for hidden window");
                    continue;
                }
            }

            if let Some(window) = reactor.window_manager.windows.get_mut(&wid) {
                window.frame_monotonic = target_frame;
            }
        }

        if animated_count > 0 {
            let low_power = power::is_low_power_mode_enabled();
            let layout_animate = reactor
                .layout_manager
                .layout_engine
                .layout_specific_animate_settings(space)
                .unwrap_or(reactor.config.settings.animate);

            if is_resize || !layout_animate || low_power {
                anim.skip_to_end();
            } else {
                anim.run();
            }
        }

        any_frame_changed
    }

    pub fn instant_layout(
        reactor: &mut Reactor,
        layout: &[(WindowId, CGRect)],
        skip_wid: Option<WindowId>,
    ) -> bool {
        let mut per_app: HashMap<pid_t, Vec<(WindowId, CGRect)>> = HashMap::default();
        let mut any_frame_changed = false;

        for &(wid, target_frame) in layout {
            // Skip applying a layout frame for the window currently being dragged.
            if skip_wid == Some(wid) {
                trace!(?wid, "Skipping layout update for window currently being dragged");
                continue;
            }

            let Some(window) = reactor.window_manager.windows.get_mut(&wid) else {
                debug!(?wid, "Skipping layout - window no longer exists");
                continue;
            };
            let target_frame = target_frame.round();
            let current_frame = window.frame_monotonic;
            if target_frame.same_as(current_frame) {
                continue;
            }
            if let Some(wsid) = window.info.sys_id {
                if reactor
                    .transaction_manager
                    .get_target_frame(wsid)
                    .is_some_and(|pending| pending.same_as(target_frame))
                {
                    trace!(?wid, ?target_frame, "Skipping redundant instant layout request");
                    continue;
                }
            }
            any_frame_changed = true;
            trace!(
                ?wid,
                ?current_frame,
                ?target_frame,
                "Instant workspace positioning"
            );

            per_app.entry(wid.pid).or_default().push((wid, target_frame));
        }

        for (pid, frames) in per_app.into_iter() {
            if frames.is_empty() {
                continue;
            }

            let Some(app_state) = reactor.app_manager.apps.get(&pid) else {
                debug!(?pid, "Skipping layout update for app - app no longer exists");
                continue;
            };

            let handle = app_state.handle.clone();

            let (first_wid, first_target) = frames[0];
            let mut txid = TransactionId::default();
            let mut has_txid = false;
            let mut txid_entries: Vec<(WindowServerId, TransactionId, CGRect)> = Vec::new();
            if let Some(window) = reactor.window_manager.windows.get_mut(&first_wid) {
                if let Some(wsid) = window.info.sys_id {
                    txid = reactor.transaction_manager.generate_next_txid(wsid);
                    has_txid = true;
                    txid_entries.push((wsid, txid, first_target));
                }
            }

            if has_txid {
                for (wid, frame) in frames.iter().skip(1) {
                    if let Some(w) = reactor.window_manager.windows.get_mut(wid) {
                        if let Some(wsid) = w.info.sys_id {
                            reactor.transaction_manager.set_last_sent_txid(wsid, txid);
                            txid_entries.push((wsid, txid, *frame));
                        }
                    }
                }
                reactor.transaction_manager.update_txid_entries(txid_entries);
            }

            let frames_to_send = frames.clone();
            if let Err(e) = handle.send(Request::SetBatchWindowFrame(frames_to_send, txid)) {
                debug!(
                    ?pid,
                    ?e,
                    "Failed to send batch frame request - app may have quit"
                );
                continue;
            }

            for (wid, target_frame) in &frames {
                if let Some(window) = reactor.window_manager.windows.get_mut(wid) {
                    window.frame_monotonic = *target_frame;
                }
            }
        }

        any_frame_changed
    }
}

#[cfg(test)]
mod tests {
    use objc2_core_foundation::{CGPoint, CGRect, CGSize};

    use super::{should_skip_scrolling_animation, size_changed, visible_width_ratio};

    fn rect(x: f64, y: f64, w: f64, h: f64) -> CGRect {
        CGRect::new(CGPoint::new(x, y), CGSize::new(w, h))
    }

    #[test]
    fn scrolling_animation_skips_hidden_edge_to_hidden_edge_moves() {
        let screen = rect(0.0, 0.0, 3840.0, 1589.0);
        let current = rect(-2200.0, 39.0, 2294.0, 1573.0);
        let target = rect(-2240.0, 39.0, 2294.0, 1573.0);
        assert!(should_skip_scrolling_animation(current, target, screen));
    }

    #[test]
    fn scrolling_animation_keeps_visible_to_hidden_moves_animated() {
        let screen = rect(0.0, 0.0, 3840.0, 1589.0);
        let current = rect(770.0, 39.0, 2294.0, 1573.0);
        let target = rect(3832.0, 39.0, 2294.0, 1573.0);
        assert!(!should_skip_scrolling_animation(current, target, screen));
    }

    #[test]
    fn scrolling_animation_keeps_hidden_to_visible_moves_animated() {
        let screen = rect(0.0, 0.0, 3840.0, 1589.0);
        let current = rect(-1719.0, 39.0, 2294.0, 1573.0);
        let target = rect(770.0, 39.0, 2294.0, 1573.0);
        assert!(!should_skip_scrolling_animation(current, target, screen));
    }

    #[test]
    fn visible_width_ratio_tracks_partially_visible_columns() {
        let screen = rect(0.0, 0.0, 3840.0, 1589.0);
        let frame = rect(-1719.0, 39.0, 2294.0, 1573.0);
        let ratio = visible_width_ratio(frame, screen);
        assert!(ratio > 0.2 && ratio < 0.3, "unexpected ratio {ratio}");
    }

    #[test]
    fn scrolling_animation_keeps_meaningfully_visible_edge_columns_animated() {
        let screen = rect(0.0, 0.0, 3840.0, 1589.0);
        let current = rect(-900.0, 39.0, 2294.0, 1573.0);
        let target = rect(770.0, 39.0, 2294.0, 1573.0);
        assert!(!should_skip_scrolling_animation(current, target, screen));
    }

    #[test]
    fn scrolling_animation_keeps_partially_visible_columns_animated_when_parking() {
        let screen = rect(0.0, 0.0, 3840.0, 1589.0);
        let current = rect(-1719.0, 39.0, 2294.0, 1573.0);
        let target = rect(-2200.0, 39.0, 2294.0, 1573.0);
        assert!(!should_skip_scrolling_animation(current, target, screen));
    }

    #[test]
    fn size_changed_only_for_real_resizes() {
        let current = rect(-1719.0, 39.0, 2294.0, 1573.0);
        let translated = rect(-2200.0, 39.0, 2294.0, 1573.0);
        let resized = rect(-2200.0, 39.0, 2200.0, 1573.0);

        assert!(!size_changed(current, translated));
        assert!(size_changed(current, resized));
    }
}
