//! Building, starting, watching and stopping the services themselves.

use anyhow::{Context, Result, bail};
use std::path::Path;
use std::time::Duration;

use crate::estate::{Estate, Resolved};
use crate::process::{self, Wait};
use crate::ui;

pub fn is_running(estate: &Estate, name: &str) -> bool {
    process::read_pid(&estate.pid_file(name)).is_some_and(process::alive)
}

pub fn pid(estate: &Estate, name: &str) -> Option<i32> {
    process::read_pid(&estate.pid_file(name)).filter(|pid| process::alive(*pid))
}

/// Some services ship no dev certificate of their own. Borrowing one keeps the
/// whole estate on the same scheme, which matters more than it sounds: a Secure
/// cookie set by one service has to be readable by the next.
pub fn ensure_cert(estate: &Estate, name: &str) -> Result<()> {
    let service = estate.service(name)?;
    let Some(source) = &service.cert_from else {
        return Ok(());
    };

    let resolved = estate.resolve(name)?;
    let files = estate.cert_files(service);
    let source_dir = estate.path(source);

    if files
        .iter()
        .all(|file| resolved.workdir.join(file).exists())
    {
        return Ok(());
    }

    if !files.iter().all(|file| source_dir.join(file).exists()) {
        ui::warn(name, "no dev certificate; will serve plain HTTP");
        return Ok(());
    }

    for file in &files {
        let link = resolved.workdir.join(file);
        // Replace rather than fail: a dangling link from a previous checkout is
        // exactly the case this has to recover from.
        let _ = std::fs::remove_file(&link);
        std::os::unix::fs::symlink(source_dir.join(file), &link)
            .with_context(|| format!("linking {}", link.display()))?;
    }

    ui::info(name, format!("linked the dev certificate from {source}"));
    Ok(())
}

pub fn build(service: &Resolved, root: &Path) -> Result<()> {
    let Some((program, args)) = service.build.split_first() else {
        return Ok(());
    };

    ui::info(&service.name, "building");
    if !process::run(program, args, root, &[])? {
        bail!("{} build failed", service.package);
    }
    Ok(())
}

pub fn start(estate: &Estate, name: &str) -> Result<bool> {
    let service = estate.resolve(name)?;

    if is_running(estate, name) {
        ui::ok(name, format!("already running on {}", service.port));
        return Ok(true);
    }

    build(&service, &estate.root)?;

    for note in &service.notes {
        ui::info(name, note);
    }

    if !service.binary.is_file() {
        bail!(
            "{} was not built to {}",
            service.package,
            service.binary.display()
        );
    }

    // Run from the service's own directory: that is where its .env, TLS
    // certificate, translations and static assets live, and every one of those
    // paths is resolved relative to the working directory.
    let log_file = estate.log_file(name);
    let pid = process::spawn_detached(
        &service.binary,
        &service.workdir,
        &service.env,
        &service.unset,
        &log_file,
    )?;
    process::write_pid(&estate.pid_file(name), pid)?;

    let health_url = service.health_url();
    let waited = process::wait_for(
        Some(pid),
        Duration::from_secs(service.start_timeout_secs),
        || healthy(&health_url),
    );

    match waited {
        Wait::Ready => {
            ui::ok(name, format!("{} (pid {pid})", service.url()));
            Ok(true)
        }
        Wait::Died => {
            ui::error(name, "exited during startup");
            ui::quote(&process::tail(&log_file, 5));
            let _ = std::fs::remove_file(estate.pid_file(name));
            Ok(false)
        }
        Wait::Timeout => {
            // Still running, just not answering yet. Leaving it up is the
            // useful answer: the log says why, and killing it would throw that
            // away along with whatever state it had reached.
            ui::warn(
                name,
                format!(
                    "started (pid {pid}) but {} did not answer - see {}",
                    service.health_path,
                    log_file.display()
                ),
            );
            Ok(true)
        }
    }
}

/// curl rather than a built-in HTTP client, because a development estate serves
/// self-signed certificates and `-k` is one flag against a pile of TLS
/// configuration that would only ever be used to ignore it.
fn healthy(url: &str) -> bool {
    matches!(
        process::capture("curl", &["-sk", "-o", "/dev/null", "--max-time", "2", url],),
        Ok((true, _))
    )
}

/// Anything a service spawned that it, not foreman, owns. Best effort: the
/// service may already be down, and a hook that fails should not stop a stop.
fn run_pre_stop_hooks(estate: &Estate, name: &str) -> Result<()> {
    let service = estate.service(name)?;
    if service.pre_stop.is_empty() || !is_running(estate, name) {
        return Ok(());
    }

    let scope = estate.scope().with("name", name).with("service", name);

    for hook in &service.pre_stop {
        if let Some(description) = &hook.description {
            ui::info(name, scope.expand(description)?);
        }
        let script = scope.expand(&hook.shell)?;
        let ok = process::shell(
            &script,
            &estate.root,
            Duration::from_secs(hook.timeout_secs),
        )?;
        if !ok {
            ui::warn(name, "pre-stop hook did not complete");
        }
        if hook.settle_secs > 0 {
            std::thread::sleep(Duration::from_secs(hook.settle_secs));
        }
    }

    Ok(())
}

/// Stops the named services. Dependencies are deliberately not expanded:
/// stopping one service should not take down everything that authenticates
/// against it.
pub fn stop(estate: &Estate, names: &[String]) -> Result<()> {
    for name in names {
        run_pre_stop_hooks(estate, name)?;

        let pid_file = estate.pid_file(name);
        match pid(estate, name) {
            Some(pid) => {
                let timeout = estate.resolve(name)?.stop_timeout_secs;
                process::terminate(pid);

                let waited =
                    process::wait_for(None, Duration::from_secs(timeout), || !process::alive(pid));
                if !matches!(waited, Wait::Ready) {
                    process::kill(pid);
                }
                ui::ok(name, "stopped");
            }
            None => ui::info(name, "not running"),
        }
        let _ = std::fs::remove_file(&pid_file);
    }

    Ok(())
}

/// Processes that can outlive the service that spawned them, reported rather
/// than killed - some of them may not be ours to kill.
pub fn report_strays(estate: &Estate) -> Result<()> {
    for warning in &estate.config.warnings {
        let pids = process::pgrep(&warning.pgrep);
        if pids.is_empty() {
            continue;
        }
        let joined = pids.join(" ");
        ui::warn(&warning.name, format!("still running: {joined}"));
        let message = estate
            .scope()
            .with("pids", &joined)
            .expand(&warning.message)?;
        ui::info(&warning.name, message);
    }
    Ok(())
}

pub fn running_services(estate: &Estate) -> Vec<String> {
    estate
        .service_names()
        .into_iter()
        .filter(|name| is_running(estate, name))
        .collect()
}
