use std::sync::mpsc;
use std::time::{Duration, Instant};

use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use tracing::{debug, trace};

use super::TransactionId;
use crate::actor::app::{AppThreadHandle, Request, SynchronizedAnimationBatch, WindowId, pid_t};
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
    jump_windows: Vec<(&'a AppThreadHandle, WindowId, CGRect, TransactionId)>,
}

impl<'a> Animation<'a> {
    pub fn new(fps: f64, duration: f64, _: AnimationEasing) -> Self {
        // When fps is 0 (or negative), use the display's native refresh rate.
        // Fall back to 60 Hz if the rate cannot be determined.
        let fps = if fps > 0.0 {
            fps
        } else {
            crate::sys::display_link::get_display_refresh_rate().unwrap_or(60.0)
        };
        Animation {
            configured_fps: fps.min(60.0),
            duration: Duration::from_secs_f64(duration.max(0.0)),
            windows: vec![],
            jump_windows: vec![],
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

    pub fn add_jump_window(
        &mut self,
        handle: &'a AppThreadHandle,
        wid: WindowId,
        target: CGRect,
        txid: TransactionId,
    ) {
        self.jump_windows.push((handle, wid, target, txid))
    }

    pub fn run(self) {
        if self.windows.is_empty() && self.jump_windows.is_empty() {
            return;
        }
        if self.windows.is_empty() {
            self.skip_to_end();
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
        let fps = self.effective_fps();
        let frame_count = if duration_secs == 0.0 {
            1
        } else {
            (duration_secs * fps).round().max(1.0) as u32
        };
        let mut next_frames = Vec::with_capacity(self.windows.len());
        let mut sent_mid_resize = false;

        for frame_idx in 1..=frame_count {
            let t = f64::from(frame_idx) / f64::from(frame_count);
            let deadline = start + Duration::from_secs_f64(duration_secs * t);
            match deadline.checked_duration_since(Instant::now()) {
                Some(remaining) => std::thread::sleep(remaining),
                // Keep the overall animation duration bounded when AX writes
                // are slow by dropping overdue intermediate frames. The final
                // frame is still delivered so we always land on the target.
                None if frame_idx != frame_count => continue,
                None => {}
            }

            let is_final = frame_idx == frame_count;
            let should_resize = is_final || (!sent_mid_resize && frame_idx * 2 >= frame_count);

            next_frames.clear();
            for (_, _, from, to, _, _) in &self.windows {
                next_frames.push(get_frame(*from, *to, t));
            }

            let mut batches_by_pid: HashMap<
                pid_t,
                (
                    &AppThreadHandle,
                    TransactionId,
                    Vec<(WindowId, CGRect)>,
                    Vec<(WindowId, CGPoint)>,
                ),
            > = HashMap::default();

            for (&(handle, wid, from, to, _, txid), rect) in self.windows.iter().zip(&next_frames) {
                let mut rect = *rect;
                // Round interpolated positions to whole pixels to prevent a
                // feedback loop: sub-pixel values get rounded by macOS, rift
                // sees the rounded frame as a change, recalculates layout with
                // a slightly different result, and the window drifts.
                rect.origin = rect.origin.round();
                // Actually don't animate size, too slow. Resize halfway through
                // and then set the size again at the end, in case it got
                // clipped during the animation.
                let entry =
                    batches_by_pid.entry(wid.pid).or_insert((handle, txid, Vec::new(), Vec::new()));
                if should_resize && size_changed(from, to) {
                    rect.size = to.size;
                    entry.2.push((wid, rect));
                } else {
                    entry.3.push((wid, rect.origin));
                }
            }
            if frame_idx == 1 {
                for &(handle, wid, target, txid) in &self.jump_windows {
                    let entry = batches_by_pid.entry(wid.pid).or_insert((
                        handle,
                        txid,
                        Vec::new(),
                        Vec::new(),
                    ));
                    entry.2.push((wid, target));
                }
            }

            let (ready_tx, ready_rx) = mpsc::channel();
            let (done_tx, done_rx) = mpsc::channel();
            let mut release_txs = Vec::new();
            let mut dispatched = 0usize;

            for (_, (handle, txid, frames, positions)) in batches_by_pid {
                let (release_tx, release_rx) = mpsc::channel();
                let request = Request::RunSynchronizedAnimationBatch(SynchronizedAnimationBatch {
                    txid,
                    frames,
                    positions,
                    ready_tx: ready_tx.clone(),
                    release_rx,
                    done_tx: done_tx.clone(),
                });
                if handle.send(request).is_ok() {
                    release_txs.push(release_tx);
                    dispatched += 1;
                }
            }

            for _ in 0..dispatched {
                if ready_rx.recv().is_err() {
                    break;
                }
            }
            for release_tx in release_txs {
                let _ = release_tx.send(());
            }
            for _ in 0..dispatched {
                if done_rx.recv().is_err() {
                    break;
                }
            }

            if should_resize && !is_final {
                sent_mid_resize = true;
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
        let mut batches_by_pid: HashMap<
            pid_t,
            (
                &AppThreadHandle,
                TransactionId,
                Vec<(WindowId, CGRect)>,
                Vec<(WindowId, CGPoint)>,
            ),
        > = HashMap::default();

        for &(handle, wid, _from, to, _, txid) in &self.windows {
            let entry =
                batches_by_pid.entry(wid.pid).or_insert((handle, txid, Vec::new(), Vec::new()));
            entry.2.push((wid, to));
        }
        for &(handle, wid, target, txid) in &self.jump_windows {
            let entry =
                batches_by_pid.entry(wid.pid).or_insert((handle, txid, Vec::new(), Vec::new()));
            entry.2.push((wid, target));
        }

        let (ready_tx, ready_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let mut release_txs = Vec::new();
        let mut dispatched = 0usize;

        for (_, (handle, txid, frames, positions)) in batches_by_pid {
            let (release_tx, release_rx) = mpsc::channel();
            let request = Request::RunSynchronizedAnimationBatch(SynchronizedAnimationBatch {
                txid,
                frames,
                positions,
                ready_tx: ready_tx.clone(),
                release_rx,
                done_tx: done_tx.clone(),
            });
            if handle.send(request).is_ok() {
                release_txs.push(release_tx);
                dispatched += 1;
            }
        }

        for _ in 0..dispatched {
            if ready_rx.recv().is_err() {
                break;
            }
        }
        for release_tx in release_txs {
            let _ = release_tx.send(());
        }
        for _ in 0..dispatched {
            if done_rx.recv().is_err() {
                break;
            }
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

fn edge_transition_needs_jump(current: CGRect, target: CGRect, screen: CGRect) -> bool {
    const MIN_VISIBLE_RATIO_FOR_SMOOTH_EDGE_ANIMATION: f64 = 0.5;

    visible_width_ratio(current, screen) < MIN_VISIBLE_RATIO_FOR_SMOOTH_EDGE_ANIMATION
        || visible_width_ratio(target, screen) < MIN_VISIBLE_RATIO_FOR_SMOOTH_EDGE_ANIMATION
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
        const RAISE_TIMEOUT_TROUBLE_TTL_MS: u64 = 20_000;
        reactor.app_manager.purge_expired_raise_timeouts(RAISE_TIMEOUT_TROUBLE_TTL_MS);
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
            let trouble_edge_jump = scrolling_screen.is_some_and(|screen| {
                edge_transition_needs_jump(current_frame, target_frame, screen)
                    && reactor
                        .app_manager
                        .is_window_recently_raise_timed_out(wid, RAISE_TIMEOUT_TROUBLE_TTL_MS)
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
                        trouble_edge_jump,
                        "Scrolling animation decision"
                    );
                }
            }

            if is_active && trouble_edge_jump {
                trace!(
                    ?wid,
                    ?current_frame,
                    ?target_frame,
                    "Jumping recently troubled edge window in synchronized batch"
                );
                animated_wids_wsids.push(wid.idx.into());
                anim.add_jump_window(&app_state.handle, wid, target_frame, txid);
                animated_count += 1;
                if let Some(wsid) = window_server_id {
                    reactor.transaction_manager.update_txid_entries([(wsid, txid, target_frame)]);
                }
            } else if is_active && !skip_edge_parked_animation {
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
        let mut per_app: HashMap<pid_t, (AppThreadHandle, Vec<(WindowId, CGRect)>)> =
            HashMap::default();
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

            let Some(app_state) = reactor.app_manager.apps.get(&wid.pid) else {
                debug!(?wid, "Skipping layout update for app - app no longer exists");
                continue;
            };
            per_app
                .entry(wid.pid)
                .or_insert_with(|| (app_state.handle.clone(), Vec::new()))
                .1
                .push((wid, target_frame));
        }

        let (ready_tx, ready_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let mut release_txs = Vec::new();
        let mut dispatched = 0usize;

        for (pid, (handle, frames)) in per_app.into_iter() {
            if frames.is_empty() {
                continue;
            }

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

            let (release_tx, release_rx) = mpsc::channel();
            let request = Request::RunSynchronizedAnimationBatch(SynchronizedAnimationBatch {
                txid,
                frames: frames.clone(),
                positions: Vec::new(),
                ready_tx: ready_tx.clone(),
                release_rx,
                done_tx: done_tx.clone(),
            });
            if let Err(e) = handle.send(request) {
                debug!(
                    ?pid,
                    ?e,
                    "Failed to send batch frame request - app may have quit"
                );
                continue;
            }
            release_txs.push(release_tx);
            dispatched += 1;

            for (wid, target_frame) in &frames {
                if let Some(window) = reactor.window_manager.windows.get_mut(wid) {
                    window.frame_monotonic = *target_frame;
                }
            }
        }

        for _ in 0..dispatched {
            if ready_rx.recv().is_err() {
                break;
            }
        }
        for release_tx in release_txs {
            let _ = release_tx.send(());
        }
        for _ in 0..dispatched {
            if done_rx.recv().is_err() {
                break;
            }
        }

        any_frame_changed
    }
}

#[cfg(test)]
mod tests {
    use objc2_core_foundation::{CGPoint, CGRect, CGSize};

    use super::{
        edge_transition_needs_jump, should_skip_scrolling_animation, size_changed,
        visible_width_ratio,
    };

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
    fn edge_transition_detection_is_separate_from_hidden_edge_skip() {
        let screen = rect(0.0, 0.0, 3840.0, 1589.0);
        let current = rect(-1719.0, 39.0, 2294.0, 1573.0);
        let target = rect(770.0, 39.0, 2294.0, 1573.0);
        assert!(edge_transition_needs_jump(current, target, screen));
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
