use crate::config::Job;
use std::io::{BufRead, BufReader, Read};
use std::process::{Command, Stdio};
use std::thread::JoinHandle;

/// Whether rsync reads `path` as a remote location (`user@host:/dir`, `host:dir`, `rsync://host/module`) rather
/// than a local directory: a `:` before the first `/`, as rsync itself decides.
#[must_use]
pub fn is_remote(path: &str) -> bool {
    path.starts_with("rsync://")
        || path
            .split('/')
            .next()
            .is_some_and(|first| first.contains(':'))
}

/// Pins `cmd` to the source directory (see `dry_run`) - only when that is a local directory; a remote source has
/// none to enter, and spawning in it fails with `ENOENT`.
fn pin_working_dir(cmd: &mut Command, src: &str) {
    if !is_remote(src) {
        cmd.current_dir(src);
    }
}

/// The remote shell to give rsync (`-e`), or `None` to leave it to rsync and the environment.
///
/// None where the user chose one (`RSYNC_RSH`), and on Windows, whose rsync may bring an `ssh` of its own. Elsewhere `ssh`
/// with a connect timeout, so a host that is off costs seconds and not the system's two minutes, and keepalives so a host that
/// goes off mid-transfer ends the transfer instead of hanging it. `probe` adds `BatchMode`: a check must never wait for a
/// password.
#[must_use]
pub fn remote_shell(probe: bool) -> Option<String> {
    if cfg!(windows) || std::env::var_os("RSYNC_RSH").is_some_and(|value| !value.is_empty()) {
        return None;
    }
    Some(if probe {
        "ssh -o BatchMode=yes -o ConnectTimeout=5 -o ServerAliveInterval=5 -o ServerAliveCountMax=2"
            .to_string()
    } else {
        "ssh -o ConnectTimeout=10 -o ServerAliveInterval=15 -o ServerAliveCountMax=4".to_string()
    })
}

/// Gives a command that talks to a remote end the shell of [`remote_shell`].
fn use_remote_shell(cmd: &mut Command, job: &Job) {
    if (is_remote(&job.src) || is_remote(&job.dest))
        && let Some(shell) = remote_shell(false)
    {
        cmd.arg("-e").arg(shell);
    }
}

/// How much a person wants to see of a sync.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Output {
    /// Every file, as it is found and as it is copied, with progress: for someone at a terminal.
    Files,
    /// Nothing per file, only the counts: for a daemon whose output goes to a journal. A first sync of a big drive is
    /// millions of files, and a journal that gets a line for each one drops the lines that matter.
    Summary,
}

/// What a dry run found, by kind.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Changes {
    pub created: usize,
    pub modified: usize,
    pub deleted: usize,
}

impl Changes {
    #[must_use]
    pub const fn total(&self) -> usize {
        self.created + self.modified + self.deleted
    }

    /// `12 new, 3 changed, 1 deleted`, leaving out what is zero.
    #[must_use]
    pub fn describe(&self) -> String {
        let parts: Vec<String> = [
            (self.created, "new"),
            (self.modified, "changed"),
            (self.deleted, "deleted"),
        ]
        .into_iter()
        .filter(|(count, _)| *count > 0)
        .map(|(count, what)| format!("{count} {what}"))
        .collect();
        if parts.is_empty() {
            "no changes".to_string()
        } else {
            parts.join(", ")
        }
    }
}

/// The dry run: what a sync of `job` would change.
///
/// The lines rsync prints are read as they arrive and counted, never kept, so a tree of millions of files costs no memory.
/// With [`Output::Files`] each change is also shown.
pub fn scan(job: &Job, output: Output) -> Result<Changes, Box<dyn std::error::Error>> {
    let files = output == Output::Files;
    if files {
        println!("Executing dry-run");
    }

    let mut cmd = Command::new("rsync");
    // Pinned rather than inherited: src and dest are already absolute-or-as-given
    // in every argument below, so this is not for resolving them - it is so
    // rsync's own `getcwd()` never depends on whatever this process's ambient
    // working directory happens to be at the moment it is spawned. Two jobs
    // never race on it, since each pins to its own source.
    pin_working_dir(&mut cmd, &job.src);
    use_remote_shell(&mut cmd, job);
    cmd.arg(if files { "-avz" } else { "-az" })
        .arg("--dry-run")
        .arg("--itemize-changes")
        .arg("--delete");

    for skip in &job.skip {
        cmd.arg(format!("--exclude={skip}"));
    }

    cmd.arg(format!("{}/", job.src));
    cmd.arg(format!("{}/", job.dest));

    let mut changes = Changes::default();
    run_streaming(&mut cmd, |line| match classify_line(line) {
        Some(Change::Delete(path)) => {
            if files {
                println!("DELETE  {path}");
            }
            changes.deleted += 1;
        }
        Some(Change::Create(path)) => {
            if let (true, Some(path)) = (files, path) {
                println!("CREATE {path}");
            }
            changes.created += 1;
        }
        Some(Change::Modify(path)) => {
            if let (true, Some(path)) = (files, path) {
                println!("MODIFY {path}");
            }
            changes.modified += 1;
        }
        None => {}
    })?;

    if files {
        println!("{}: {} total changes", job.desc, changes.total());
    }

    Ok(changes)
}

/// Whether a sync of `job` would change anything, shown file by file (the REPL and `pulley run`).
pub fn dry_run(job: &Job) -> Result<bool, Box<dyn std::error::Error>> {
    Ok(scan(job, Output::Files)?.total() > 0)
}

/// One `--itemize-changes` line's meaning, or `None` for a line dry-run
/// doesn't report as a change (a directory entering sync unchanged, a
/// progress line, etc).
#[derive(Debug, PartialEq, Eq)]
pub enum Change {
    Delete(String),
    /// `None` when the line had no trailing path to show - still a change to
    /// count, just nothing to print.
    Create(Option<String>),
    Modify(Option<String>),
}

#[must_use]
pub fn classify_line(line: &str) -> Option<Change> {
    if line.starts_with("*deleting") {
        return Some(Change::Delete(line.replace("*deleting ", "")));
    }
    if line.starts_with(">f+++++++++") || line.starts_with("<f+++++++++") || line.starts_with("cd+")
    {
        return Some(Change::Create(
            line.split_once(' ').map(|(_, p)| p.to_string()),
        ));
    }
    if line.starts_with(">f") {
        return Some(Change::Modify(
            line.split_once(' ').map(|(_, p)| p.to_string()),
        ));
    }
    None
}

/// The sync itself, shown file by file with progress (the REPL and `pulley run`).
pub fn update(job: &Job) -> Result<(), Box<dyn std::error::Error>> {
    println!("Executing update");
    run_command_async(&mut update_command(job, Output::Files))
}

/// The sync itself without a line per file or any progress: what a daemon runs. Fails with the tail of rsync's own complaints.
pub fn update_quietly(job: &Job) -> Result<(), Box<dyn std::error::Error>> {
    run_streaming(&mut update_command(job, Output::Summary), |_| {})
}

fn update_command(job: &Job, output: Output) -> Command {
    let mut cmd = Command::new("rsync");
    // See the matching comment in `scan`.
    pin_working_dir(&mut cmd, &job.src);
    use_remote_shell(&mut cmd, job);
    cmd.arg(match output {
        Output::Files => "-avzu",
        Output::Summary => "-azu",
    });
    if output == Output::Files {
        cmd.arg("--progress");
    }

    if job.delete {
        cmd.arg("--delete");
    }

    for skip in &job.skip {
        cmd.arg(format!("--exclude={skip}"));
    }

    cmd.arg(format!("{}/", job.src));
    cmd.arg(format!("{}/", job.dest));
    cmd
}

/// How many of rsync's last lines of complaint an error keeps.
const STDERR_TAIL: usize = 20;

/// Runs `command`, giving each line it prints to `on_line` as it arrives.
///
/// Lines are trimmed and directory lines (which end in `/`) are left out. Nothing is kept: not the lines, and of rsync's
/// complaints only the last few, for the error.
///
/// # Errors
/// If rsync cannot be started, or exits with a failure: `rsync exited with <status>: <what it complained of>`.
pub fn run_streaming(
    command: &mut Command,
    mut on_line: impl FnMut(&str),
) -> Result<(), Box<dyn std::error::Error>> {
    let mut process = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let stdout = process.stdout.take().unwrap();
    let stderr = process.stderr.take().unwrap();

    let stderr_handle = std::thread::spawn(move || {
        let mut tail: std::collections::VecDeque<String> = std::collections::VecDeque::new();
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            if tail.len() == STDERR_TAIL {
                tail.pop_front();
            }
            tail.push_back(line);
        }
        tail
    });

    for line in BufReader::new(stdout).lines().map_while(Result::ok) {
        let line = line.trim();
        if !line.ends_with('/') {
            on_line(line);
        }
    }

    let status = process.wait()?;
    let tail = stderr_handle.join().unwrap();

    if !status.success() {
        return Err(format!(
            "rsync exited with {status}: {}",
            tail.into_iter().collect::<Vec<_>>().join(" | ")
        )
        .into());
    }

    Ok(())
}

pub fn run_command(command: &mut Command) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut lines = Vec::new();
    run_streaming(command, |line| lines.push(line.to_string()))?;
    Ok(lines)
}

pub fn run_command_async(command: &mut Command) -> Result<(), Box<dyn std::error::Error>> {
    fn thread_spawn(
        child: impl Read + Send + 'static,
        sender: std::sync::mpsc::Sender<String>,
    ) -> JoinHandle<()> {
        std::thread::spawn(move || {
            let reader_err = BufReader::new(child);
            for line in reader_err.lines().map_while(Result::ok) {
                if sender.send(line).is_err() {
                    break;
                }
            }
        })
    }

    let mut process = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let stdout = process.stdout.take().unwrap();
    let stderr = process.stderr.take().unwrap();

    let (stdout_tx, stdout_rx) = std::sync::mpsc::channel();
    let (stderr_tx, stderr_rx) = std::sync::mpsc::channel();

    let stdout_handle = thread_spawn(stdout, stdout_tx);
    let stderr_handle = thread_spawn(stderr, stderr_tx);

    let stdout_thread = std::thread::spawn(move || {
        for line in stdout_rx {
            eprintln!("{line}");
        }
    });

    let stderr_thread = std::thread::spawn(move || {
        for line in stderr_rx {
            eprintln!("{line}");
        }
    });

    let status = process.wait()?;

    stdout_handle.join().unwrap();
    stderr_handle.join().unwrap();
    stdout_thread.join().unwrap();
    stderr_thread.join().unwrap();

    if !status.success() {
        return Err(format!("rsync exited with {status}").into());
    }

    Ok(())
}
