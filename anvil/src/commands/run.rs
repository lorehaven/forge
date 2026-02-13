use anyhow::{Context, Result};
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

    println!(
        "Starting serve mode (watch + rebuild){}",
        package_ref
            .map(|p| format!(" for package '{p}'"))
            .unwrap_or_default()
    );

    run_command(build_command(package_ref, true), "build")?;
    let mut child = spawn_run_child(package_ref)?;
    let mut last_snapshot = file_snapshot(".")?;

    loop {
        thread::sleep(interval);

        if let Some(status) = child
            .try_wait()
            .context("Failed to check run process status")?
        {
            println!("Run process exited with status {status}. Waiting for file changes...");
        }

        let snapshot = file_snapshot(".")?;
        if snapshot == last_snapshot {
            continue;
        }
        last_snapshot = snapshot;

        println!("Detected file change. Rebuilding and restarting...");
        stop_child_if_running(&mut child)?;
        run_command(build_command(package_ref, true), "build")?;
        child = spawn_run_child(package_ref)?;
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
    cmd.stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    cmd.spawn().context("Failed to spawn cargo run process")
}

fn stop_child_if_running(child: &mut Child) -> Result<()> {
    if child.try_wait()?.is_none() {
        child.kill().context("Failed to stop running process")?;
        let _ = child.wait();
    }
    Ok(())
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
    let ignored_dir_names = [".git", "target", ".ferrous", ".idea", ".vscode"];
    !path
        .components()
        .any(|c| ignored_dir_names.iter().any(|name| c.as_os_str() == *name))
}
