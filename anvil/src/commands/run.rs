use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use std::collections::BTreeMap;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use walkdir::WalkDir;

use crate::util::run_command;

pub fn run(package: Option<&str>, serve: bool, watch_interval_ms: u64) -> Result<()> {
    if serve {
        run_serve_mode(package, watch_interval_ms)
    } else {
        run_build_and_run(package)
    }
}

fn run_build_and_run(package: Option<&str>) -> Result<()> {
    run_command(build_command(package, true), "build")?;
    run_command(run_command_for_package(package), "run")
}

fn run_serve_mode(package: Option<&str>, watch_interval_ms: u64) -> Result<()> {
    let package_ref = package;
    let interval = Duration::from_millis(watch_interval_ms.max(200));
    let mut auto_rebuild = true;

    println!(
        "Starting serve mode (watch + rebuild){}",
        package_ref
            .map(|p| format!(" for package '{p}'"))
            .unwrap_or_default()
    );
    println!("Hotkeys: 'r' = rebuild now, 'R' = toggle auto-rebuild, 'q'/'Q'/'e'/'E' = quit");

    let mut child: Option<Child> = None;
    rebuild_and_restart(package_ref, &mut child, "Initial build");
    let mut last_snapshot = file_snapshot(".")?;

    loop {
        thread::sleep(interval);

        for event in read_hotkeys()? {
            match event {
                HotkeyEvent::Rebuild => {
                    println!("Manual rebuild requested.");
                    rebuild_and_restart(package_ref, &mut child, "Manual rebuild");
                    last_snapshot = file_snapshot(".")?;
                }
                HotkeyEvent::ToggleAutoRebuild => {
                    auto_rebuild = !auto_rebuild;
                    println!(
                        "Auto-rebuild: {}",
                        if auto_rebuild { "enabled" } else { "disabled" }
                    );
                }
                HotkeyEvent::Quit => {
                    println!("Quit requested.");
                    stop_child_if_running(&mut child)?;
                    return Ok(());
                }
            }
        }

        if let Some(proc) = child.as_mut()
            && let Some(status) = proc
                .try_wait()
                .context("Failed to check run process status")?
        {
            println!("Run process exited with status {status}. Waiting for file changes...");
            child = None;
        }

        let snapshot = file_snapshot(".")?;
        if snapshot == last_snapshot {
            continue;
        }
        last_snapshot = snapshot;

        if !auto_rebuild {
            println!("Detected file change (auto-rebuild disabled).");
            continue;
        }

        println!("Detected file change. Rebuilding...");
        rebuild_and_restart(package_ref, &mut child, "Auto rebuild");
    }
}

fn build_command(package: Option<&str>, all_features: bool) -> Command {
    let mut cmd = Command::new("cargo");
    cmd.arg("build");
    if all_features {
        cmd.arg("--all-features");
    }
    if let Some(pkg) = package {
        cmd.arg("--package").arg(pkg);
    }
    cmd
}

fn run_command_for_package(package: Option<&str>) -> Command {
    let mut cmd = Command::new("cargo");
    cmd.arg("run");
    if let Some(pkg) = package {
        cmd.arg("--package").arg(pkg);
    }
    cmd
}

fn spawn_run_child(package: Option<&str>) -> Result<Child> {
    let mut cmd = run_command_for_package(package);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    cmd.spawn().context("Failed to spawn cargo run process")
}

fn stop_child_if_running(child: &mut Option<Child>) -> Result<()> {
    if let Some(proc) = child.as_mut()
        && proc.try_wait()?.is_none()
    {
        proc.kill().context("Failed to stop running process")?;
        let _ = proc.wait();
    }
    *child = None;
    Ok(())
}

fn rebuild_and_restart(package: Option<&str>, child: &mut Option<Child>, reason: &str) {
    match run_command(build_command(package, true), "build") {
        Ok(()) => {
            if let Err(err) = stop_child_if_running(child) {
                eprintln!("Failed to stop previous process: {err}");
            }
            match spawn_run_child(package) {
                Ok(new_child) => {
                    *child = Some(new_child);
                    println!("{reason}: build succeeded, process running.");
                }
                Err(err) => {
                    eprintln!("{reason}: build succeeded but failed to start process: {err}");
                }
            }
        }
        Err(err) => {
            eprintln!("{reason}: build failed: {err}");
            eprintln!("Waiting for next change or manual rebuild.");
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum HotkeyEvent {
    Rebuild,
    ToggleAutoRebuild,
    Quit,
}

fn read_hotkeys() -> Result<Vec<HotkeyEvent>> {
    let mut events = Vec::new();
    enable_raw_mode().context("Failed to enable terminal raw mode")?;

    while let Ok(ready) = event::poll(Duration::from_millis(0)) {
        if !ready {
            break;
        }
        if let Ok(Event::Key(key)) = event::read() {
            if key.kind != KeyEventKind::Press {
                continue;
            }

            let hotkey = match key.code {
                KeyCode::Char('r') => Some(HotkeyEvent::Rebuild),
                KeyCode::Char('R') => Some(HotkeyEvent::ToggleAutoRebuild),
                KeyCode::Char('q' | 'Q' | 'e' | 'E') => Some(HotkeyEvent::Quit),
                _ => None,
            };
            if let Some(hotkey) = hotkey {
                clear_echoed_hotkey();
                events.push(hotkey);
            }
        }
    }
    disable_raw_mode().ok();
    Ok(events)
}

fn clear_echoed_hotkey() {
    // If a key was echoed by terminal line discipline between polls, erase it.
    // This is best-effort and no-op when nothing was echoed.
    eprint!("\u{8} \u{8}");
}

fn file_snapshot(root: impl AsRef<Path>) -> Result<BTreeMap<String, u128>> {
    let mut snapshot = BTreeMap::new();
    let walker = WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| should_watch_path(e.path()));

    for entry in walker {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if !should_watch_file(path) {
            continue;
        }

        let metadata = entry.metadata()?;
        let modified = metadata
            .modified()
            .unwrap_or(SystemTime::UNIX_EPOCH)
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();

        let key = path.to_string_lossy().to_string();
        snapshot.insert(key, modified);
    }

    Ok(snapshot)
}

fn should_watch_path(path: &Path) -> bool {
    let ignored_dir_names = [
        ".git", "target", ".ferrous", ".idea", ".vscode", ".direnv", ".cache", "storage", "dist",
        "tmp", ".tmp",
    ];
    !path
        .components()
        .any(|c| ignored_dir_names.iter().any(|name| c.as_os_str() == *name))
}

fn should_watch_file(path: &Path) -> bool {
    let Some(file_name) = path.file_name().and_then(|s| s.to_str()) else {
        return false;
    };
    if matches!(
        file_name,
        "Cargo.toml" | "Cargo.lock" | "rust-toolchain.toml"
    ) {
        return true;
    }

    matches!(
        path.extension().and_then(|s| s.to_str()),
        Some("rs" | "toml" | "yaml" | "yml")
    )
}
