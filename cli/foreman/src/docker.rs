//! The containers the estate sits on.
//!
//! Foreman does not manage their data or their lifecycle beyond up and down: a
//! development database is meant to be disposable, and `--rm` says so.

use anyhow::{Result, bail};
use std::time::Duration;

use crate::config::Container;
use crate::process;
use crate::ui;
use crate::vars::Scope;

fn require_docker() -> Result<()> {
    if process::capture("docker", &["version", "--format", "{{.Client.Version}}"]).is_err() {
        bail!("docker not found on PATH");
    }
    Ok(())
}

pub fn is_running(container: &Container) -> bool {
    let filter = format!("name=^{}$", container.container_name());
    matches!(
        process::capture("docker", &["ps", "-q", "-f", &filter]),
        Ok((true, output)) if !output.is_empty()
    )
}

pub fn start(container: &Container, scope: &Scope) -> Result<()> {
    require_docker()?;

    if is_running(container) {
        ui::ok(&container.name, "already running");
        return Ok(());
    }

    ui::info(&container.name, format!("starting {}", container.image));

    let mut args = vec![
        "run".to_string(),
        "--rm".to_string(),
        "-d".to_string(),
        "--name".to_string(),
        container.container_name().to_string(),
    ];
    for (key, value) in scope.expand_map(&container.env)? {
        args.push("-e".to_string());
        args.push(format!("{key}={value}"));
    }
    for port in &container.ports {
        args.push("-p".to_string());
        args.push(scope.expand(port)?);
    }
    args.extend(scope.expand_all(&container.args)?);
    args.push(container.image.clone());

    let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();
    let (ok, output) = process::capture("docker", &borrowed)?;
    if !ok {
        bail!("failed to start {} ({output})", container.name);
    }

    wait_until_ready(container)
}

/// A container with a published port is listening long before it is answering.
/// The readiness command runs inside the container, so what it reports is the
/// service's own opinion rather than the port's.
fn wait_until_ready(container: &Container) -> Result<()> {
    let address = container.address.as_deref().unwrap_or("");
    let ready_message = if address.is_empty() {
        "ready".to_string()
    } else {
        format!("ready on {address}")
    };

    if container.ready.is_empty() {
        ui::ok(&container.name, ready_message);
        return Ok(());
    }

    ui::info(&container.name, "waiting for connections");
    let name = container.container_name().to_string();
    let probe = container.ready.clone();

    let waited = process::wait_for(
        None,
        Duration::from_secs(container.ready_timeout_secs),
        move || {
            let mut args = vec!["exec", name.as_str()];
            args.extend(probe.iter().map(String::as_str));
            matches!(process::capture("docker", &args), Ok((true, _)))
        },
    );

    match waited {
        process::Wait::Ready => {
            ui::ok(&container.name, ready_message);
            Ok(())
        }
        _ => bail!("{} did not become ready", container.name),
    }
}

pub fn stop(container: &Container) {
    let name = container.container_name();
    match process::capture("docker", &["stop", name]) {
        Ok((true, _)) => ui::ok(&container.name, "stopped"),
        _ => ui::info(&container.name, "not running"),
    }
}

pub fn status(container: &Container) {
    if is_running(container) {
        let address = container.address.as_deref().unwrap_or("");
        let message = if address.is_empty() {
            "running".to_string()
        } else {
            format!("running on {address}")
        };
        ui::ok(&container.name, message);
    } else {
        ui::warn(&container.name, "not running");
    }
}
