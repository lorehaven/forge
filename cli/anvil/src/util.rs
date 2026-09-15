use anyhow::{Context, Result};
use quench_cli::prelude::{DIM, RESET, Tone, print_status};
use std::collections::VecDeque;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::Instant;

/// How many lines of a command's output to fall back to showing when it
/// isn't streamed live - enough to see the actual compiler/registry error
/// without reprinting an entire multi-thousand-line build log.
const FAILURE_TAIL_LINES: usize = 80;

/// How anvil surfaces a shelled-out command's stdout/stderr.
///
/// Set once from the top-level `--silent`/`--tail` flags via
/// [`set_output_mode`] and read by every [`run_command`] /
/// [`run_command_streamed`] call for the life of the process.
#[derive(Debug, Clone, Copy, Default)]
pub enum OutputMode {
    /// Stream every line live, exactly as running the command by hand would.
    #[default]
    Full,
    /// Print nothing live; only anvil's own status lines, plus a short tail
    /// of output when the command fails.
    Silent,
    /// Print nothing live; print the last N lines once the command finishes,
    /// success or failure.
    Tail(usize),
}

static OUTPUT_MODE: OnceLock<OutputMode> = OnceLock::new();

/// Must be called at most once, before any command runs (`main` sets this
/// from the parsed CLI flags). Later calls are ignored.
pub fn set_output_mode(mode: OutputMode) {
    let _ = OUTPUT_MODE.set(mode);
}

fn output_mode() -> OutputMode {
    OUTPUT_MODE.get().copied().unwrap_or_default()
}

#[must_use]
pub fn log_file_path(operation: &str) -> PathBuf {
    let slug: String = operation
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let slug = slug.trim_matches('-');
    let slug = if slug.is_empty() { "operation" } else { slug };
    PathBuf::from("target/anvil-logs").join(format!("{slug}.log"))
}

/// A bounded ring buffer of the most recent output lines, kept so
/// [`OutputMode::Silent`] and [`OutputMode::Tail`] can show a tail even when
/// there is no on-disk log to read it back from (see
/// [`run_command_streamed`]).
struct TailCapture {
    lines: Mutex<VecDeque<String>>,
    cap: usize,
}

impl TailCapture {
    fn new(cap: usize) -> Self {
        Self {
            lines: Mutex::new(VecDeque::with_capacity(cap)),
            cap,
        }
    }

    fn push(&self, line: String) {
        let mut lines = self
            .lines
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if lines.len() == self.cap {
            lines.pop_front();
        }
        lines.push_back(line);
    }

    fn snapshot(&self) -> Vec<String> {
        self.lines
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .cloned()
            .collect()
    }
}

/// Copies a child's output stream line by line to whichever of the log file,
/// the terminal, and the tail buffer are wired up for this run.
fn pump_stream<R: std::io::Read + Send + 'static>(
    reader: R,
    log_writer: Option<Arc<Mutex<File>>>,
    stream_live: bool,
    to_stderr: bool,
    tail: Arc<TailCapture>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        for line in BufReader::new(reader).lines() {
            let Ok(line) = line else { break };

            if let Some(writer) = &log_writer
                && let Ok(mut file) = writer.lock()
            {
                let _ = writeln!(file, "{line}");
            }

            if stream_live {
                if to_stderr {
                    eprintln!("{line}");
                } else {
                    println!("{line}");
                }
            }

            tail.push(line);
        }
    })
}

fn print_tail_lines(lines: &[String], n: usize, source: &str) {
    if lines.is_empty() {
        return;
    }
    let start = lines.len().saturating_sub(n);
    let shown = &lines[start..];

    eprintln!(
        "{DIM}--- last {} line(s) of {source} ---{RESET}",
        shown.len()
    );
    for line in shown {
        eprintln!("{line}");
    }
    eprintln!("{DIM}--- end of log ({source}) ---{RESET}");
}

/// Shared implementation behind [`run_command`] and [`run_command_streamed`].
/// `log_to_file` controls whether output is also captured to
/// `target/anvil-logs/<operation>.log`; both behave the same with respect to
/// the current [`OutputMode`] otherwise.
fn run_command_with_logging(mut cmd: Command, operation: &str, log_to_file: bool) -> Result<()> {
    print_status(
        Tone::Info,
        "anvil",
        &format!("running {operation} operation..."),
    );

    let mode = output_mode();
    let stream_live = matches!(mode, OutputMode::Full);
    let tail_n = match mode {
        OutputMode::Tail(n) => n,
        OutputMode::Silent | OutputMode::Full => FAILURE_TAIL_LINES,
    };

    let log_path = log_file_path(operation);
    let log_writer = if log_to_file {
        if let Some(parent) = log_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create log directory {}", parent.display()))?;
        }
        let file = File::create(&log_path)
            .with_context(|| format!("Failed to create log file {}", log_path.display()))?;
        Some(Arc::new(Mutex::new(file)))
    } else {
        None
    };

    let tail = Arc::new(TailCapture::new(tail_n));

    let start = Instant::now();
    let mut child = cmd
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("Failed to execute {operation} command"))?;

    let stdout = child.stdout.take().expect("stdout was piped");
    let stderr = child.stderr.take().expect("stderr was piped");

    let out_handle = pump_stream(stdout, log_writer.clone(), stream_live, false, tail.clone());
    let err_handle = pump_stream(stderr, log_writer, stream_live, true, tail.clone());

    let status = child
        .wait()
        .with_context(|| format!("Failed to execute {operation} command"))?;
    let _ = out_handle.join();
    let _ = err_handle.join();
    let elapsed = start.elapsed();

    let source = log_path
        .to_str()
        .map_or_else(|| operation.to_string(), str::to_string);

    if !status.success() {
        print_status(
            Tone::Error,
            "anvil",
            &format!("{operation} operation failed with status: {status}"),
        );
        if !stream_live {
            print_tail_lines(&tail.snapshot(), tail_n, &source);
        }
        if log_to_file {
            anyhow::bail!(
                "{operation} operation failed with status: {status} (full log: {})",
                log_path.display()
            );
        }
        anyhow::bail!("{operation} operation failed with status: {status}");
    }

    if matches!(mode, OutputMode::Tail(_)) {
        print_tail_lines(&tail.snapshot(), tail_n, &source);
    }

    print_status(
        Tone::Success,
        "anvil",
        &format!("{operation} operation completed successfully ({elapsed:.2?})"),
    );
    Ok(())
}

/// Runs `cmd`, capturing its output to `target/anvil-logs/<operation>.log`.
///
/// By default (no `--silent`/`--tail` flag) the output is also streamed live
/// to anvil's own stdout/stderr as it arrives, exactly like running the
/// command by hand. `--silent` suppresses that and only shows a short tail on
/// failure; `--tail[=N]` shows the last N lines once the command finishes,
/// success or failure.
pub fn run_command(cmd: Command, operation: &str) -> Result<()> {
    run_command_with_logging(cmd, operation, true)
}

/// Like [`run_command`], but never writes an on-disk log.
///
/// Only the current [`OutputMode`] governs what's shown, and in
/// `Silent`/`Tail` modes the tail comes from an in-memory buffer rather than
/// a log file. The captured-to-disk form is wrong for checks whose entire
/// output *is* the result - `cargo clippy`, `cargo machete`, `cargo audit`.
/// There is no on-disk log for these runs; they are cheap to repeat.
pub fn run_command_streamed(cmd: Command, operation: &str) -> Result<()> {
    run_command_with_logging(cmd, operation, false)
}

pub fn print_log_tail(log_path: &PathBuf) {
    let Ok(contents) = fs::read_to_string(log_path) else {
        return;
    };
    let lines: Vec<String> = contents.lines().map(str::to_string).collect();
    print_tail_lines(&lines, FAILURE_TAIL_LINES, &log_path.display().to_string());
}
