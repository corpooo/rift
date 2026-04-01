use std::future::Future;
use std::path::PathBuf;
use std::process;
use std::ptr;

use clap::{Parser, Subcommand};
use nix::libc;
use objc2::MainThreadMarker;
use objc2_application_services::AXUIElement;
use rift_wm::actor::config::ConfigActor;
use rift_wm::actor::config_watcher::ConfigWatcher;
use rift_wm::actor::event_tap::EventTap;
use rift_wm::actor::menu_bar::Menu;
use rift_wm::actor::mission_control::MissionControlActor;
use rift_wm::actor::mission_control_observer::NativeMissionControl;
use rift_wm::actor::notification_center::NotificationCenter;
use rift_wm::actor::process::ProcessActor;
use rift_wm::actor::reactor::{self, Reactor};
use rift_wm::actor::stack_line::StackLine;
use rift_wm::actor::window_notify as window_notify_actor;
use rift_wm::actor::wm_controller::{self, WmController};
use rift_wm::common::config::{Config, config_file, restore_file, session_state_file};
use rift_wm::common::log;
use rift_wm::common::util::execute_startup_commands;
use rift_wm::ipc;
use rift_wm::layout_engine::LayoutEngine;
use rift_wm::model::tx_store::WindowTxStore;
use rift_wm::sys::accessibility::ensure_accessibility_permission;
use rift_wm::sys::app::running_apps;
use rift_wm::sys::executor::Executor;
use rift_wm::sys::mach::init_window_sub_level_server_port;
use rift_wm::sys::screen::{CoordinateConverter, displays_have_separate_spaces};
use rift_wm::sys::service::{ServiceCommands, handle_service_command};
use rift_wm::sys::skylight::{
    CGEnableEventStateCombining, CGSEventType, CGSetLocalEventsSuppressionInterval, KnownCGSEvent,
};
use tokio::join;

embed_plist::embed_info_plist!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/Info.plist"));

#[derive(Parser)]
struct Cli {
    /// Only run the window manager on the current space.
    #[arg(long)]
    one: bool,

    /// Disable new spaces by default.
    ///
    /// Ignored if --one is used.
    #[arg(long)]
    default_disable: bool,

    /// Disable animations.
    #[arg(long)]
    no_animate: bool,

    /// No-op compatibility check for the deprecated restore file path.
    #[arg(long)]
    validate: bool,

    /// Deprecated no-op flag retained for CLI compatibility.
    #[arg(long)]
    restore: bool,

    /// Record reactor events to the specified file path. Overwrites the file if
    /// exists.
    #[arg(long)]
    record: Option<PathBuf>,

    /// Path to configuration file to use (overrides default).
    #[arg(long, value_name = "PATH")]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Manage the launchd service for rift
    Service {
        #[command(subcommand)]
        service: ServiceCommands,
    },
}

/// this is okay because there is no recovery mechanism for actors
/// so we want to immediately exit (and most likely restart since
/// rift runs as a service most of the time)
async fn supervise(name: &'static str, fut: impl Future<Output = ()>) {
    fut.await;
    panic!("{name} exited");
}

fn install_shutdown_signal_handler(events_tx: reactor::Sender) {
    unsafe fn make_signal_set() -> libc::sigset_t {
        let mut set = unsafe { std::mem::zeroed::<libc::sigset_t>() };
        unsafe {
            libc::sigemptyset(&mut set);
            libc::sigaddset(&mut set, libc::SIGTERM);
            libc::sigaddset(&mut set, libc::SIGINT);
        }
        set
    }

    let mask_result = unsafe {
        let set = make_signal_set();
        libc::pthread_sigmask(libc::SIG_BLOCK, &set, ptr::null_mut())
    };
    if mask_result != 0 {
        eprintln!("Failed to install shutdown signal mask: {}", mask_result);
        return;
    }

    std::thread::spawn(move || {
        let mut signal_number = 0;
        let wait_result = unsafe {
            let set = make_signal_set();
            libc::sigwait(&set, &mut signal_number)
        };
        if wait_result == 0 {
            events_tx.send(reactor::Event::Command(reactor::Command::Reactor(
                reactor::ReactorCommand::SaveAndExit,
            )));
        }
    });
}

fn main() {
    sigpipe::reset();
    let opt = Cli::parse();

    if let Some(Commands::Service { service }) = &opt.command {
        match handle_service_command(service) {
            Ok(msg) => {
                println!("{}", msg);
                process::exit(0);
            }
            Err(e) => {
                eprintln!("{}", e);
                process::exit(1);
            }
        }
    }

    if std::env::var_os("RUST_BACKTRACE").is_none() {
        // SAFETY: We are single threaded at this point.
        unsafe { std::env::set_var("RUST_BACKTRACE", "1") };
    }
    log::init_logging();
    install_panic_hook();

    let mtm = MainThreadMarker::new().unwrap();
    {
        use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
        let app = NSApplication::sharedApplication(mtm);
        let _ = app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
        app.finishLaunching();
        NSApplication::load();
    }

    ensure_accessibility_permission();
    init_window_sub_level_server_port();

    if !displays_have_separate_spaces() {
        eprintln!(
            "Rift detected that the macOS setting \"Displays have separate Spaces\" \
is disabled. Rift currently requires this setting to be enabled. \
Enable it in System Settings > Desktop & Dock (Mission Control) and restart Rift."
        );
        std::process::exit(1);
    }

    let config_path = opt.config.clone().unwrap_or_else(|| config_file());
    let mut config = if config_path.exists() {
        Config::read(&config_path).unwrap()
    } else {
        Config::default()
    };
    config.settings.animate &= !opt.no_animate;
    config.settings.default_disable |= opt.default_disable;

    if opt.validate {
        return;
    }

    execute_startup_commands(&config.settings.run_on_start);

    let (broadcast_tx, broadcast_rx) = rift_wm::actor::channel();
    let session_path = session_state_file();
    let startup_app_order = running_apps(None).map(|(pid, _)| pid).collect::<Vec<_>>();
    let session_exists = session_path.exists() || restore_file().exists();
    let layout = if session_path.exists() {
        LayoutEngine::load(
            session_path.clone(),
            &config.virtual_workspaces,
            &config.settings.layout,
            Some(broadcast_tx.clone()),
        )
        .unwrap_or_else(|_| {
            LayoutEngine::new(
                &config.virtual_workspaces,
                &config.settings.layout,
                Some(broadcast_tx.clone()),
            )
        })
    } else if restore_file().exists() {
        LayoutEngine::load(
            restore_file(),
            &config.virtual_workspaces,
            &config.settings.layout,
            Some(broadcast_tx.clone()),
        )
        .unwrap_or_else(|_| {
            LayoutEngine::new(
                &config.virtual_workspaces,
                &config.settings.layout,
                Some(broadcast_tx.clone()),
            )
        })
    } else {
        let mut seeded_workspace_config = config.virtual_workspaces.clone();
        seeded_workspace_config.default_workspace_count = 1;
        seeded_workspace_config.default_workspace = 0;
        seeded_workspace_config.workspace_names.truncate(1);
        LayoutEngine::new(
            &seeded_workspace_config,
            &config.settings.layout,
            Some(broadcast_tx.clone()),
        )
    };
    let (event_tap_tx, event_tap_rx) = rift_wm::actor::channel();
    let (menu_tx, menu_rx) = rift_wm::actor::channel();
    let (stack_line_tx, stack_line_rx) = rift_wm::actor::channel();
    let (wnd_tx, wnd_rx) = rift_wm::actor::channel();
    let window_tx_store = WindowTxStore::new();
    let reactor = Reactor::spawn(
        config.clone(),
        layout,
        reactor::Record::new(opt.record.as_deref()),
        event_tap_tx.clone(),
        broadcast_tx.clone(),
        menu_tx.clone(),
        stack_line_tx.clone(),
        Some((wnd_tx.clone(), window_tx_store.clone())),
        opt.one,
        startup_app_order,
        !session_exists,
    );
    let events_tx = reactor.sender();
    install_shutdown_signal_handler(events_tx.clone());

    let config_tx =
        ConfigActor::spawn_with_path(config.clone(), events_tx.clone(), config_path.clone());

    ConfigWatcher::spawn(config_tx.clone(), config.clone(), config_path.clone());

    let wn_actor = window_notify_actor::WindowNotify::new(
        events_tx.clone(),
        wnd_rx,
        &[
            CGSEventType::Known(KnownCGSEvent::SpaceWindowDestroyed),
            CGSEventType::Known(KnownCGSEvent::SpaceWindowCreated),
            CGSEventType::Known(KnownCGSEvent::SpaceCreated),
            CGSEventType::Known(KnownCGSEvent::SpaceDestroyed),
            //CGSEventType::Known(KnownCGSEvent::WindowMoved),
            //CGSEventType::Known(KnownCGSEvent::WindowResized),
        ],
        Some(window_tx_store.clone()),
    );

    let server_state = match ipc::run_mach_server(reactor.clone(), config_tx.clone()) {
        Ok(state) => state,
        Err(err) => {
            eprintln!("{}", err);
            process::exit(1);
        }
    };

    let mach_bridge_rx = broadcast_rx;

    let server_state_for_bridge = server_state.clone();
    std::thread::spawn(move || {
        let mut rx = mach_bridge_rx;
        let server_state = server_state_for_bridge;
        loop {
            match rx.blocking_recv() {
                Some((_span, event)) => {
                    let state = server_state.read();
                    state.publish(event);
                }
                None => {
                    break;
                }
            }
        }
    });

    let wm_config = wm_controller::Config {
        restore_file: restore_file(),
        config: config.clone(),
    };
    let (mc_tx, mc_rx) = rift_wm::actor::channel();
    let (_mc_native_tx, mc_native_rx) = rift_wm::actor::channel();
    let (wm_controller, wm_controller_sender) = WmController::new(
        wm_config,
        events_tx.clone(),
        event_tap_tx.clone(),
        stack_line_tx.clone(),
        mc_tx.clone(),
        Some(window_tx_store.clone()),
    );

    let _ = events_tx.send(reactor::Event::RegisterWmSender(wm_controller_sender.clone()));

    let notification_center = NotificationCenter::new(wm_controller_sender.clone());

    let process_actor = ProcessActor::new(wm_controller_sender.clone());

    let event_tap = EventTap::new(
        config.clone(),
        events_tx.clone(),
        event_tap_rx,
        Some(wm_controller_sender.clone()),
        Some(stack_line_tx.clone()),
    );
    let menu = Menu::new(
        config.clone(),
        menu_rx,
        events_tx.clone(),
        config_tx.clone(),
        mtm,
    );
    let stack_line = StackLine::new(
        config.clone(),
        stack_line_rx,
        mtm,
        events_tx.clone(),
        CoordinateConverter::default(),
    );

    let mission_control = MissionControlActor::new(config.clone(), mc_rx, reactor.clone(), mtm);
    let mission_control_native = NativeMissionControl::new(events_tx.clone(), mc_native_rx);

    if config.settings.default_disable {
        println!(
            "NOTICE: by default rift starts in a deactivated state.
            you must activate it by using the toggle_spaces_activated command.
            by default this is bound to Alt+Z but can be changed in the config file."
        );
    }

    unsafe { AXUIElement::new_system_wide().set_messaging_timeout(1.0) };

    CGSetLocalEventsSuppressionInterval(0.0);
    CGEnableEventStateCombining(false);

    Executor::run_main(mtm, async move {
        join!(
            supervise("wm_controller", wm_controller.run()),
            supervise(
                "notification_center",
                notification_center.watch_for_notifications()
            ),
            supervise("event_tap", event_tap.run()),
            supervise("menu", menu.run()),
            supervise("stack_line", stack_line.run()),
            supervise("window_notify", wn_actor.run()),
            supervise("mc_native", mission_control_native.run()),
            supervise("mission_control", mission_control.run()),
            supervise("process_actor", process_actor.run()),
        );
    });
}

#[cfg(panic = "unwind")]
fn install_panic_hook() {
    // Abort on panic instead of propagating panics to the main thread.
    // See Cargo.toml for why we don't use panic=abort everywhere.
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        original_hook(info);
        std::process::abort();
    }));
}

#[cfg(not(panic = "unwind"))]
fn install_panic_hook() {}
