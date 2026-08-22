//! Pid files, signals and the small amount of waiting that goes with them.
//!
//! Services are launched from their built binary rather than through `cargo
//! run`, because `stop` needs a real pid: killing a `cargo run` parent leaves
//! the service running and holding its port.

use anyhow::{Context, Result, bail};
use std::fs::File;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Is there a process with this pid, and may we signal it?
pub fn alive(pid: i32) -> bool {
    // Signal 0 performs the permission and existence checks without delivering
    // anything, which is exactly the question being asked.
    unsafe { libc::kill(pid, 0) == 0 }
}

pub fn terminate(pid: i32) {
    unsafe { libc::kill(pid, libc::SIGTERM) };
}

pub fn kill(pid: i32) {
    unsafe { libc::kill(pid, libc::SIGKILL) };
}

pub fn read_pid(path: &Path) -> Option<i32> {
    std::fs::read_to_string(path)
        .ok()?
        .trim()
        .parse::<i32>()
        .ok()
}

pub fn write_pid(path: &Path, pid: i32) -> Result<()> {
    std::fs::write(path, format!("{pid}\n")).with_context(|| format!("writing {}", path.display()))
}

/// Starts a process that outlives foreman.
///
/// Its own process group is the point: without it a Ctrl-C aimed at foreman
/// reaches every service through the terminal's foreground group, and a start
/// interrupted halfway would take down the services it had already brought up.
pub fn spawn_detached(
    program: &Path,
    workdir: &Path,
    env: &[(String, String)],
    unset: &[String],
    log_file: &Path,
) -> Result<i32> {
    let log = File::create(log_file).with_context(|| format!("creating {}", log_file.display()))?;
    let errors = log.try_clone()?;

    let mut command = Command::new(program);
    command
        .current_dir(workdir)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(errors));

    for (key, value) in env {
        command.env(key, value);
    }
    // Last, so `unset` always wins - it exists to take a name away, including
    // one inherited from the shell that started foreman.
    for name in unset {
        command.env_remove(name);
    }
    command.process_group(0);

    let child = command
        .spawn()
        .with_context(|| format!("starting {}", program.display()))?;

    Ok(child.id() as i32)
}

/// Runs a command to completion with its output on the terminal.
pub fn run(
    program: &str,
    args: &[String],
    workdir: &Path,
    env: &[(String, String)],
) -> Result<bool> {
    let mut command = Command::new(program);
    command.args(args).current_dir(workdir);
    for (key, value) in env {
        command.env(key, value);
    }

    let status = command
        .status()
        .with_context(|| format!("running {program}"))?;
    Ok(status.success())
}

/// Runs a command to completion, keeping its output. Used for the questions
/// foreman asks of docker and pgrep, where the answer matters and the noise
/// does not.
pub fn capture(program: &str, args: &[&str]) -> Result<(bool, String)> {
    let output = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("running {program}"))?;

    Ok((
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).trim().to_string(),
    ))
}

/// `sh -c`, so a hook can be a pipeline or a loop. Hooks talk to services over
/// their APIs, and that is rarely one command.
pub fn shell(script: &str, workdir: &Path, timeout: Duration) -> Result<bool> {
    let mut child = Command::new("sh")
        .arg("-c")
        .arg(script)
        .current_dir(workdir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("running hook")?;

    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status.success());
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(false);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Pids matching a `pgrep -f` pattern. A pattern that matches nothing is not a
/// failure, it is the good case.
pub fn pgrep(pattern: &str) -> Vec<String> {
    match capture("pgrep", &["-f", pattern]) {
        Ok((_, output)) => output
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect(),
        Err(_) => Vec::new(),
    }
}

/// Polls `check` until it answers true, the process dies, or time runs out.
pub enum Wait {
    Ready,
    Timeout,
    Died,
}

pub fn wait_for(pid: Option<i32>, timeout: Duration, mut check: impl FnMut() -> bool) -> Wait {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(pid) = pid
            && !alive(pid)
        {
            return Wait::Died;
        }
        if check() {
            return Wait::Ready;
        }
        if Instant::now() >= deadline {
            return Wait::Timeout;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// The last few lines of a file, for reporting a service that died on the way
/// up without printing its whole log.
pub fn tail(path: &Path, lines: usize) -> String {
    let Ok(text) = std::fs::read_to_string(path) else {
        return String::new();
    };
    let all: Vec<&str> = text.lines().collect();
    all[all.len().saturating_sub(lines)..].join("\n")
}

/// Follows a file the way `tail -f` does, until interrupted.
pub fn follow(path: &Path) -> Result<()> {
    if !path.is_file() {
        bail!("no log at {}", path.display());
    }
    let status = Command::new("tail")
        .arg("-f")
        .arg(path)
        .status()
        .context("running tail")?;
    // Ctrl-C is how you leave `tail -f`, so a non-zero exit here is the normal
    // way out rather than a failure worth reporting.
    let _ = status;
    Ok(())
}
