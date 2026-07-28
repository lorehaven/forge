//! Running a job as child processes of this service.
//!
//! Steps run in order, in the checkout, and the first one to fail ends the job.
//! Output is read line by line off both pipes and given a sequence number by
//! the job task rather than by the readers, so stdout and stderr interleave in
//! the order they arrived and every subscriber sees the same order.
//!
//! This executor runs whatever the repository asked for with this service's
//! privileges. That is the whole reason repositories are registered explicitly
//! and fork pull requests are refused by default.

use crate::domain::Status;
use crate::executors::engine::{
    ExecError, Handle, JobExecutor, JobSpec, JobState, LogChunk, LogTail, StepState, Stream,
};
use crate::secrets::Redactor;
use crate::steps;
use crate::workspace::Workspace;
use async_trait::async_trait;
use chrono::Utc;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::{broadcast, mpsc, watch};
use tokio::time::{Duration, Instant};

/// How many log lines a subscriber may fall behind before it is dropped.
///
/// A dropped subscriber is not a lost log: the lines are in `history`, which is
/// what a reader re-reads after a lag. The channel exists to make output live,
/// not to store it.
const LOG_CHANNEL_CAPACITY: usize = 1024;

/// Bound on the reader-to-job-task queue. Backpressure here is the right
/// behaviour: a step producing output faster than it can be recorded should be
/// slowed by its own pipe, not have its output dropped.
const LINE_QUEUE_CAPACITY: usize = 256;

#[derive(Clone)]
struct Running {
    state: Arc<Mutex<JobState>>,
    history: Arc<Mutex<Vec<LogChunk>>>,
    publisher: broadcast::Sender<LogChunk>,
    cancel: watch::Sender<bool>,
}

#[derive(Default)]
pub struct NativeExecutor {
    jobs: Arc<Mutex<HashMap<String, Running>>>,
}

impl NativeExecutor {
    pub fn new() -> Self {
        Self::default()
    }

    fn get(&self, handle: &Handle) -> Result<Running, ExecError> {
        self.jobs
            .lock()
            .expect("job registry poisoned")
            .get(handle.as_str())
            .cloned()
            .ok_or_else(|| ExecError::UnknownHandle(handle.clone()))
    }
}

#[async_trait]
impl JobExecutor for NativeExecutor {
    fn name(&self) -> &'static str {
        "native"
    }

    async fn start(&self, spec: &JobSpec, workspace: &Workspace) -> Result<Handle, ExecError> {
        if spec.steps.is_empty() {
            return Err(ExecError::NoSteps {
                job: spec.name.clone(),
            });
        }

        if spec.image.is_some() {
            // Not an error: a pipeline that names an image is perfectly valid,
            // and refusing it here would make every such repository unbuildable
            // on a single-node deployment. Saying so beats the confusion of a
            // job that quietly ran somewhere other than the image says.
            tracing::warn!(
                "job {} names image {:?}, which the native executor ignores; \
                 steps run with the toolchain conveyor itself has",
                spec.name,
                spec.image
            );
        }

        // Resolved before anything is spawned, so a quoting mistake in step
        // four does not surface after steps one to three have already run.
        let mut planned = Vec::with_capacity(spec.steps.len());
        for step in &spec.steps {
            planned.push((
                step.kind().to_string(),
                step.command().to_string(),
                steps::argv(step)?,
            ));
        }

        let state = Arc::new(Mutex::new(JobState {
            status: Status::Queued,
            exit_code: None,
            error: None,
            started_at: None,
            finished_at: None,
            steps: planned
                .iter()
                .enumerate()
                .map(|(ordinal, (kind, command, _))| StepState {
                    ordinal,
                    kind: kind.clone(),
                    command: spec.redactor.apply(command),
                    status: Status::Queued,
                    exit_code: None,
                    started_at: None,
                    finished_at: None,
                })
                .collect(),
        }));

        let (publisher, _) = broadcast::channel(LOG_CHANNEL_CAPACITY);
        let (cancel, cancel_rx) = watch::channel(false);

        let running = Running {
            state: state.clone(),
            history: Arc::new(Mutex::new(Vec::new())),
            publisher: publisher.clone(),
            cancel,
        };

        self.jobs
            .lock()
            .expect("job registry poisoned")
            .insert(spec.id.clone(), running.clone());

        let context = JobContext {
            name: spec.name.clone(),
            root: workspace.root().to_path_buf(),
            env: spec.env.clone().into_iter().collect(),
            timeout: spec.timeout,
            planned,
            running,
            cancel: cancel_rx,
            redactor: spec.redactor.clone(),
        };
        tokio::spawn(run_job(context));

        Ok(Handle::new(spec.id.clone()))
    }

    async fn poll(&self, handle: &Handle) -> Result<JobState, ExecError> {
        let running = self.get(handle)?;
        let state = running.state.lock().expect("job state poisoned").clone();
        Ok(state)
    }

    async fn logs(&self, handle: &Handle) -> Result<LogTail, ExecError> {
        let running = self.get(handle)?;
        // Subscribe before snapshotting: the other order leaves a window in
        // which a line is written into neither the snapshot nor the channel.
        let live = running.publisher.subscribe();
        let history = running
            .history
            .lock()
            .expect("log history poisoned")
            .clone();
        Ok(LogTail { history, live })
    }

    async fn cancel(&self, handle: &Handle) -> Result<(), ExecError> {
        let running = self.get(handle)?;
        let _ = running.cancel.send(true);
        Ok(())
    }

    async fn forget(&self, handle: &Handle) -> Result<(), ExecError> {
        self.jobs
            .lock()
            .expect("job registry poisoned")
            .remove(handle.as_str());
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// The job task
// ---------------------------------------------------------------------------

struct JobContext {
    name: String,
    root: PathBuf,
    env: Vec<(String, String)>,
    timeout: Duration,
    /// Step kind, the command as written, and the argv to spawn.
    planned: Vec<(String, String, Vec<String>)>,
    running: Running,
    cancel: watch::Receiver<bool>,
    redactor: Redactor,
}

/// What ended a step.
enum StepOutcome {
    Passed,
    Failed(Option<i32>),
    Cancelled,
    TimedOut,
    NotStarted(String),
}

async fn run_job(mut context: JobContext) {
    let deadline = Instant::now() + context.timeout;
    let mut emitter = Emitter {
        seq: 0,
        running: context.running.clone(),
        redactor: context.redactor.clone(),
    };

    {
        let mut state = context.running.state.lock().expect("job state poisoned");
        state.status = Status::Running;
        state.started_at = Some(Utc::now());
    }

    let mut job_status = Status::Success;
    let mut job_exit_code = None;
    let mut job_error = None;

    for (ordinal, (_, command, argv)) in context.planned.iter().enumerate() {
        if *context.cancel.borrow() {
            finish_step(&context.running, ordinal, Status::Cancelled, None);
            job_status = Status::Cancelled;
            job_error = Some("cancelled before the step started".to_string());
            break;
        }

        begin_step(&context.running, ordinal);
        emitter.emit(Stream::Stdout, format!("$ {command}"));

        let outcome = run_step(
            argv,
            &context.root,
            &context.env,
            deadline,
            &mut context.cancel,
            &mut emitter,
        )
        .await;

        match outcome {
            StepOutcome::Passed => {
                finish_step(&context.running, ordinal, Status::Success, Some(0));
                job_exit_code = Some(0);
            }
            StepOutcome::Failed(code) => {
                finish_step(&context.running, ordinal, Status::Failed, code);
                job_status = Status::Failed;
                job_exit_code = code;
                emitter.emit(
                    Stream::Stderr,
                    format!(
                        "step failed with exit code {}",
                        code.map_or_else(|| "signal".to_string(), |c| c.to_string())
                    ),
                );
                break;
            }
            StepOutcome::Cancelled => {
                finish_step(&context.running, ordinal, Status::Cancelled, None);
                job_status = Status::Cancelled;
                job_error = Some("cancelled".to_string());
                emitter.emit(Stream::Stderr, "step cancelled".to_string());
                break;
            }
            StepOutcome::TimedOut => {
                finish_step(&context.running, ordinal, Status::Failed, None);
                // A timeout is a failed build, not a cancelled one: nobody
                // asked for it to stop and the code was not shown to work.
                job_status = Status::Failed;
                job_error = Some(format!(
                    "job exceeded its timeout of {}s",
                    context.timeout.as_secs()
                ));
                emitter.emit(
                    Stream::Stderr,
                    format!("timed out after {}s", context.timeout.as_secs()),
                );
                break;
            }
            StepOutcome::NotStarted(reason) => {
                finish_step(&context.running, ordinal, Status::Failed, None);
                job_status = Status::Failed;
                job_error = Some(reason.clone());
                emitter.emit(Stream::Stderr, reason);
                break;
            }
        }
    }

    let mut state = context.running.state.lock().expect("job state poisoned");
    // Everything after the step that ended the job never ran. Leaving them
    // `Queued` would make a finished job look like it was still going.
    for step in &mut state.steps {
        if step.status == Status::Queued {
            step.status = Status::Skipped;
        }
    }
    state.status = job_status;
    state.exit_code = job_exit_code;
    state.error = job_error;
    state.finished_at = Some(Utc::now());

    tracing::info!("job {} finished: {}", context.name, state.status);
}

async fn run_step(
    argv: &[String],
    root: &Path,
    env: &[(String, String)],
    deadline: Instant,
    cancel: &mut watch::Receiver<bool>,
    emitter: &mut Emitter,
) -> StepOutcome {
    let mut command = Command::new(&argv[0]);
    command
        .args(&argv[1..])
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // Without this a step survives the task being dropped, keeps the
        // checkout directory busy, and outlives the run that started it.
        .kill_on_drop(true);

    for (key, value) in env {
        command.env(key, value);
    }

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            return StepOutcome::NotStarted(format!("could not run `{}`: {error}", argv[0]));
        }
    };

    let (lines, mut queue) = mpsc::channel::<(Stream, String)>(LINE_QUEUE_CAPACITY);
    if let Some(stdout) = child.stdout.take() {
        tokio::spawn(pump(stdout, Stream::Stdout, lines.clone()));
    }
    if let Some(stderr) = child.stderr.take() {
        tokio::spawn(pump(stderr, Stream::Stderr, lines.clone()));
    }
    // The job task's own sender has to go, or the queue never closes and the
    // drain below waits forever.
    drop(lines);

    let outcome = loop {
        tokio::select! {
            // Biased so output already in the queue is recorded before the
            // exit is noticed; otherwise a fast step's last lines are drained
            // after its status is decided, which reads as output from nowhere.
            biased;

            Some((stream, line)) = queue.recv() => emitter.emit(stream, line),

            status = child.wait() => break match status {
                Ok(status) if status.success() => StepOutcome::Passed,
                Ok(status) => StepOutcome::Failed(status.code()),
                Err(error) => StepOutcome::NotStarted(format!("could not wait for step: {error}")),
            },

            _ = cancel.changed() => {
                if *cancel.borrow() {
                    let _ = child.kill().await;
                    break StepOutcome::Cancelled;
                }
            }

            () = tokio::time::sleep_until(deadline) => {
                let _ = child.kill().await;
                break StepOutcome::TimedOut;
            }
        }
    };

    while let Some((stream, line)) = queue.recv().await {
        emitter.emit(stream, line);
    }

    outcome
}

async fn pump<R>(reader: R, stream: Stream, lines: mpsc::Sender<(Stream, String)>)
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut reader = BufReader::new(reader).lines();
    // `next_line` splits on newlines and gives up on invalid UTF-8. Build
    // output is text; a step that emits a binary blob on stdout gets its output
    // truncated rather than taking the job down with it.
    while let Ok(Some(line)) = reader.next_line().await {
        if lines.send((stream, line)).await.is_err() {
            break;
        }
    }
}

/// Assigns sequence numbers and fans a line out to history and subscribers.
///
/// Redaction happens here, at the one point every line passes through, rather
/// than at each of the places that produce one. A line that never reaches this
/// struct is never recorded either.
struct Emitter {
    seq: u64,
    running: Running,
    redactor: Redactor,
}

impl Emitter {
    fn emit(&mut self, stream: Stream, line: String) {
        let chunk = LogChunk {
            seq: self.seq,
            stream,
            line: self.redactor.apply(&line),
            at: Utc::now(),
        };
        self.seq += 1;

        self.running
            .history
            .lock()
            .expect("log history poisoned")
            .push(chunk.clone());

        // Fails only when nobody is subscribed, which is the normal case for a
        // job nobody is watching.
        let _ = self.running.publisher.send(chunk);
    }
}

fn begin_step(running: &Running, ordinal: usize) {
    let mut state = running.state.lock().expect("job state poisoned");
    if let Some(step) = state.steps.get_mut(ordinal) {
        step.status = Status::Running;
        step.started_at = Some(Utc::now());
    }
}

fn finish_step(running: &Running, ordinal: usize, status: Status, exit_code: Option<i32>) {
    let mut state = running.state.lock().expect("job state poisoned");
    if let Some(step) = state.steps.get_mut(ordinal) {
        step.status = status;
        step.exit_code = exit_code;
        step.finished_at = Some(Utc::now());
    }
}
