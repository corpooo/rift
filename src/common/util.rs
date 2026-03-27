use std::env;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

use tracing::{error, info, trace, warn};

pub fn parse_command(command: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current_part = String::new();
    let mut in_quotes = false;
    let mut chars = command.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '\'' | '"' => {
                if in_quotes {
                    in_quotes = false;
                } else {
                    in_quotes = true;
                }
            }
            ' ' | '\t' if !in_quotes => {
                if !current_part.is_empty() {
                    parts.push(current_part.clone());
                    current_part.clear();
                }
            }
            '\\' if in_quotes => {
                if let Some(next_ch) = chars.next() {
                    match next_ch {
                        'n' => current_part.push('\n'),
                        't' => current_part.push('\t'),
                        'r' => current_part.push('\r'),
                        '\\' => current_part.push('\\'),
                        '\'' => current_part.push('\''),
                        '"' => current_part.push('"'),
                        _ => {
                            current_part.push('\\');
                            current_part.push(next_ch);
                        }
                    }
                } else {
                    current_part.push('\\');
                }
            }
            _ => {
                current_part.push(ch);
            }
        }
    }

    if !current_part.is_empty() {
        parts.push(current_part);
    }

    parts
}

pub fn execute_startup_commands(commands: &[String]) {
    if commands.is_empty() {
        return;
    }

    trace!("Executing {} startup commands", commands.len());

    for (i, command) in commands.iter().enumerate() {
        trace!("Executing startup command {}: {}", i + 1, command);

        let parts = parse_command(command);
        if parts.is_empty() {
            error!("Empty startup command at index {}", i);
            continue;
        }

        let (cmd, args) = parts.split_first().unwrap();

        let cmd_owned = cmd.to_string();
        let args_owned: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        let command_str = command.clone();

        std::thread::spawn(move || {
            let output = std::process::Command::new(&cmd_owned).args(&args_owned).output();

            match output {
                Ok(output) => {
                    if output.status.success() {
                        trace!("Startup command completed successfully: {}", command_str);
                    } else {
                        error!(
                            "Startup command failed with status {}: {}",
                            output.status, command_str
                        );
                        if !output.stderr.is_empty() {
                            error!("stderr: {}", String::from_utf8_lossy(&output.stderr));
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to execute startup command '{}': {}", command_str, e);
                }
            }
        });
    }
}

pub fn launch_ui_client_if_available() {
    if env_flag_is_false("RIFT_UI_AUTOLAUNCH") {
        trace!("Skipping UI auto-launch because RIFT_UI_AUTOLAUNCH is disabled");
        return;
    }

    if rift_ui_is_running() {
        trace!("Skipping UI auto-launch because RiftUI is already running");
        return;
    }

    let Some(target) = resolve_ui_client_target() else {
        trace!("No Rift UI client target found for auto-launch");
        return;
    };

    std::thread::spawn(move || match launch_ui_target(&target) {
        Ok(()) => info!("Launched Rift UI client: {}", target.display()),
        Err(err) => warn!("Failed to auto-launch Rift UI client '{}': {}", target.display(), err),
    });
}

fn env_flag_is_false(name: &str) -> bool {
    matches!(
        env::var(name)
            .ok()
            .map(|value| value.trim().to_ascii_lowercase())
            .as_deref(),
        Some("0" | "false" | "no" | "off")
    )
}

fn rift_ui_is_running() -> bool {
    Command::new("/usr/bin/pgrep")
        .args(["-x", "RiftUI"])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn resolve_ui_client_target() -> Option<PathBuf> {
    if let Some(explicit) = env::var_os("RIFT_UI_PATH") {
        let path = PathBuf::from(explicit);
        if path.exists() {
            return Some(path);
        }
    }

    let mut search_roots = Vec::new();
    search_roots.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    if let Ok(current_exe) = env::current_exe() {
        let mut current = current_exe.parent().map(Path::to_path_buf);
        while let Some(dir) = current {
            if !search_roots.contains(&dir) {
                search_roots.push(dir.clone());
            }
            current = dir.parent().map(Path::to_path_buf);
        }
    }

    if let Some(home) = env::var_os("HOME") {
        let home = PathBuf::from(home);
        search_roots.push(home.clone());
        search_roots.push(home.join("Applications"));
    }
    search_roots.push(PathBuf::from("/Applications"));

    for root in search_roots {
        for suffix in [
            PathBuf::from("swift-client/RiftUI/.build/arm64-apple-macosx/debug/RiftUI"),
            PathBuf::from("swift-client/RiftUI/.build/debug/RiftUI"),
            PathBuf::from("Applications/RiftUI.app"),
            PathBuf::from("Applications/Rift UI.app"),
            PathBuf::from("RiftUI.app"),
            PathBuf::from("Rift UI.app"),
        ] {
            let candidate = root.join(&suffix);
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }

    None
}

fn launch_ui_target(target: &Path) -> std::io::Result<()> {
    if target.extension() == Some(OsStr::new("app")) {
        Command::new("/usr/bin/open")
            .args(["-g", target.to_string_lossy().as_ref()])
            .spawn()?;
        return Ok(());
    }

    Command::new(target)
        .env("RIFT_UI_AUTOLAUNCH", "0")
        .env("RIFT_UI_AUTOLAUNCHED", "1")
        .spawn()?;
    Ok(())
}
