//! Running a job as a `batch/v1` Job in a cluster - isolation from conveyor's own privileges,
//! which is what makes `CONVEYOR_ALLOW_FORK_PR` defensible. The pod fetches its own commit.

use crate::domain::Status;
use crate::executors::engine::{
    ExecError, Handle, JobExecutor, JobSpec, JobState, LogChunk, LogTail, StepState, Stream,
};
use crate::executors::export::Exports;
use crate::executors::manifest::{self, FILE_MARKER, STEP_MARKER, Settings};
use crate::secrets::Redactor;
use crate::steps;
use crate::workspace::Workspace;
use quench_core::async_trait;
use quench_core::stream::StreamExt;
use quench_core::time::Time;
use quench_k8s::{
    Api, ApiResource, Client, DeleteParams, Error as KubeError, ListParams, LogParams, Object,
    PostParams, lines,
};
use quench_serde::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

const LOG_CHANNEL_CAPACITY: usize = 1024;

/// How long to wait for the pod to exist before giving up on following its log.
const POD_WAIT_ATTEMPTS: u32 = 120;

#[derive(Debug, Clone)]
struct Running {
    state: Arc<Mutex<JobState>>,
    history: Arc<Mutex<Vec<LogChunk>>>,
    publisher: broadcast::Sender<LogChunk>,
    /// The Job/Secret this handle owns, so `forget` can remove them.
    object_name: String,
    has_secret: bool,
}

pub struct KubernetesExecutor {
    client: Client,
    namespace: String,
    settings: Settings,
    jobs: Arc<Mutex<HashMap<String, Running>>>,
}

impl KubernetesExecutor {
    /// Uses the in-cluster service account, or a kubeconfig outside one.
    pub fn connect() -> Result<Self, String> {
        let client =
            Client::infer().map_err(|error| format!("could not reach the cluster: {error}"))?;

        let namespace = quench_core::env::get_or("CONVEYOR_K8S_NAMESPACE", "")
            .trim()
            .to_string();
        let namespace = if namespace.is_empty() {
            // In-pod, this file names the namespace; outside one, `default` is the best guess.
            std::fs::read_to_string("/var/run/secrets/kubernetes.io/serviceaccount/namespace")
                .map_or_else(|_| "default".to_string(), |value| value.trim().to_string())
        } else {
            namespace
        };

        quench_log::info!("kubernetes executor: namespace {namespace}");

        Ok(Self {
            client,
            namespace,
            settings: Settings::from_env(),
            jobs: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    fn get(&self, handle: &Handle) -> Result<Running, ExecError> {
        self.jobs
            .lock()
            .expect("job registry poisoned")
            .get(handle.as_str())
            .cloned()
            .ok_or_else(|| ExecError::UnknownHandle(handle.clone()))
    }

    fn jobs_api(&self) -> Api {
        self.client.namespaced(&ApiResource::job(), &self.namespace)
    }

    fn secrets_api(&self) -> Api {
        self.client
            .namespaced(&ApiResource::secret(), &self.namespace)
    }

    fn pods_api(&self) -> Api {
        self.client.namespaced(&ApiResource::pod(), &self.namespace)
    }
}

#[async_trait]
impl JobExecutor for KubernetesExecutor {
    fn name(&self) -> &'static str {
        "kubernetes"
    }

    async fn start(&self, spec: &JobSpec, workspace: &Workspace) -> Result<Handle, ExecError> {
        if spec.steps.is_empty() {
            return Err(ExecError::NoSteps {
                job: spec.name.clone(),
            });
        }

        if spec.source.is_none() {
            return Err(ExecError::Unsupported {
                executor: "kubernetes",
                what: "run a job with no source to fetch: the pod has no access to conveyor's own checkout"
                    .to_string(),
            });
        }

        // Resolved before submitting, so a bad quote in step four isn't discovered mid-run.
        let mut commands = Vec::with_capacity(spec.steps.len());
        for step in &spec.steps {
            commands.push(steps::argv(step)?);
        }

        let built = manifest::build(spec, &commands, &self.namespace, &self.settings);

        // Secret first: a pod whose `envFrom` names a missing Secret stays pending, not failing.
        if let Some(secret) = &built.secret {
            self.secrets_api()
                .create(secret, &PostParams::default())
                .await
                .map_err(|error| ExecError::Unsupported {
                    executor: "kubernetes",
                    what: format!("create the job's secret: {error}"),
                })?;
        }

        self.jobs_api()
            .create(&built.job, &PostParams::default())
            .await
            .map_err(|error| ExecError::Unsupported {
                executor: "kubernetes",
                what: format!("create the job: {error}"),
            })?;

        let state = Arc::new(Mutex::new(JobState {
            status: Status::Running,
            exit_code: None,
            error: None,
            started_at: Some(Time::now()),
            finished_at: None,
            steps: spec
                .steps
                .iter()
                .enumerate()
                .map(|(ordinal, step)| StepState {
                    ordinal,
                    kind: step.kind().to_string(),
                    command: spec.redactor.apply(step.command()),
                    status: Status::Queued,
                    exit_code: None,
                    started_at: None,
                    finished_at: None,
                })
                .collect(),
        }));

        let (publisher, _) = broadcast::channel(LOG_CHANNEL_CAPACITY);
        let running = Running {
            state,
            history: Arc::new(Mutex::new(Vec::new())),
            publisher,
            object_name: built.name.clone(),
            has_secret: built.secret.is_some(),
        };

        self.jobs
            .lock()
            .expect("job registry poisoned")
            .insert(spec.id.clone(), running.clone());

        tokio::spawn(watch(
            self.pods_api(),
            self.jobs_api(),
            built.name,
            spec.name.clone(),
            spec.redactor.clone(),
            running,
            (Exports::new(&spec.exports), workspace.root().to_path_buf()),
        ));

        Ok(Handle::new(spec.id.clone()))
    }

    async fn poll(&self, handle: &Handle) -> Result<JobState, ExecError> {
        let running = self.get(handle)?;
        let state = running.state.lock().expect("job state poisoned").clone();
        Ok(state)
    }

    async fn logs(&self, handle: &Handle) -> Result<LogTail, ExecError> {
        let running = self.get(handle)?;
        // Subscribe before snapshotting (as native does), or a line can reach neither.
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

        // Background propagation, so the pod goes with the Job instead of being orphaned.
        let params = DeleteParams::background();
        if let Err(error) = self.jobs_api().delete(&running.object_name, &params).await {
            quench_log::warn!("could not delete job {}: {error}", running.object_name);
        }

        let mut state = running.state.lock().expect("job state poisoned");
        if !state.status.is_terminal() {
            state.status = Status::Cancelled;
            state.error = Some("cancelled".to_string());
            state.finished_at = Some(Time::now());
            for step in &mut state.steps {
                if step.status == Status::Queued {
                    step.status = Status::Skipped;
                } else if step.status == Status::Running {
                    step.status = Status::Cancelled;
                }
            }
        }
        Ok(())
    }

    async fn forget(&self, handle: &Handle) -> Result<(), ExecError> {
        let Ok(running) = self.get(handle) else {
            return Ok(());
        };

        let params = DeleteParams::background();
        if let Err(error) = self.jobs_api().delete(&running.object_name, &params).await
            && !is_gone(&error)
        {
            quench_log::warn!("could not delete job {}: {error}", running.object_name);
        }

        if running.has_secret
            && let Err(error) = self
                .secrets_api()
                .delete(&running.object_name, &DeleteParams::default())
                .await
            && !is_gone(&error)
        {
            // Error, not warning: a secret left behind is a credential left behind.
            quench_log::error!(
                "could not delete the secret for job {}: {error}",
                running.object_name
            );
        }

        self.jobs
            .lock()
            .expect("job registry poisoned")
            .remove(handle.as_str());
        Ok(())
    }
}

/// Already deleted, which is a success for a delete.
fn is_gone(error: &KubeError) -> bool {
    error.is_not_found()
}

// Watching.
/// Follows the job's pod, recording its output and its outcome.
async fn watch(
    pods: Api,
    jobs: Api,
    object_name: String,
    job_name: String,
    redactor: Redactor,
    running: Running,
    exports: (Exports, std::path::PathBuf),
) {
    let Some(pod_name) = await_pod(&pods, &object_name).await else {
        finish(
            &running,
            Status::Failed,
            None,
            Some(format!("no pod was scheduled for {object_name}")),
        );
        return;
    };

    // Before the verdict: the worker collects from the workspace as soon as the job reads as finished.
    follow_logs(&pods, &pod_name, &redactor, &running, exports).await;

    // Log ending doesn't settle the verdict; the Job's own status decides pass/fail.
    let outcome = await_completion(&jobs, &object_name).await;
    match outcome {
        Outcome::Succeeded => finish(&running, Status::Success, Some(0), None),
        Outcome::Failed(reason) => finish(&running, Status::Failed, None, reason),
        Outcome::Vanished => finish(
            &running,
            Status::Cancelled,
            None,
            Some("the cluster job was deleted".to_string()),
        ),
    }

    quench_log::info!(
        "job {job_name} finished: {}",
        running.state.lock().expect("job state poisoned").status
    );
}

/// Waits for the Job's pod to appear and start.
async fn await_pod(pods: &Api, object_name: &str) -> Option<String> {
    let selector = ListParams::default().labels(&format!("job-name={object_name}"));

    for _ in 0..POD_WAIT_ATTEMPTS {
        if let Ok(list) = pods.list(&selector).await
            && let Some(pod) = list.items.first()
        {
            let phase = pod
                .pointer("/status/phase")
                .and_then(Value::as_str)
                .unwrap_or("Pending");

            // `Pending` covers image pulls/init container, neither producing log output yet.
            if phase != "Pending" {
                return Object(pod).name().map(str::to_string);
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
    None
}

/// Streams the work container's output into the shared history and channel.
async fn follow_logs(
    pods: &Api,
    pod_name: &str,
    redactor: &Redactor,
    running: &Running,
    (mut exports, root): (Exports, std::path::PathBuf),
) {
    let params = LogParams {
        container: Some("steps".to_string()),
        follow: true,
        ..LogParams::default()
    };

    let mut lines = match pods.log_stream(pod_name, &params).await {
        Ok(stream) => lines(stream),
        Err(error) => {
            quench_log::warn!("could not follow logs for {pod_name}: {error}");
            return;
        }
    };

    let mut seq: u64 = 0;

    while let Some(Ok(line)) = lines.next().await {
        // A file the job hands back: reassembled below, never shown in the log.
        if let Some(rest) = line.strip_prefix(FILE_MARKER) {
            exports.accept(rest);
            continue;
        }

        // Conveyor's own bookkeeping marker, not build output - consumed, not recorded.
        if let Some(ordinal) = line.trim().strip_prefix(STEP_MARKER) {
            if let Ok(ordinal) = ordinal.trim().parse::<usize>() {
                advance(running, ordinal);
            }
            continue;
        }

        let chunk = LogChunk {
            seq,
            stream: Stream::Stdout,
            line: redactor.apply(&line),
            at: Time::now(),
        };
        seq += 1;

        running
            .history
            .lock()
            .expect("log history poisoned")
            .push(chunk.clone());
        let _ = running.publisher.send(chunk);
    }

    if !exports.is_empty() {
        for problem in exports.write(&root) {
            quench_log::warn!("job output from {pod_name}: {problem}");
        }
    }
}

/// Marks the previous step done and this one running.
fn advance(running: &Running, ordinal: usize) {
    let mut state = running.state.lock().expect("job state poisoned");
    let now = Time::now();

    if ordinal > 0
        && let Some(previous) = state.steps.get_mut(ordinal - 1)
        && previous.status == Status::Running
    {
        previous.status = Status::Success;
        previous.exit_code = Some(0);
        previous.finished_at = Some(now);
    }

    if let Some(step) = state.steps.get_mut(ordinal) {
        step.status = Status::Running;
        step.started_at = Some(now);
    }
}

enum Outcome {
    Succeeded,
    Failed(Option<String>),
    Vanished,
}

async fn await_completion(jobs: &Api, object_name: &str) -> Outcome {
    loop {
        match jobs.get(object_name).await {
            Ok(job) => {
                let count = |field: &str| {
                    job.pointer(&format!("/status/{field}"))
                        .and_then(Value::as_u64)
                        .unwrap_or(0)
                };
                if count("succeeded") > 0 {
                    return Outcome::Succeeded;
                }
                if count("failed") > 0 {
                    let reason = job
                        .pointer("/status/conditions")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .find(|condition| {
                            condition.get("type").and_then(Value::as_str) == Some("Failed")
                        })
                        .and_then(|condition| condition.get("message")?.as_str())
                        .map(str::to_string);
                    return Outcome::Failed(reason);
                }
            }
            // Deleted underneath us - what a cancel looks like from here.
            Err(error) if is_gone(&error) => return Outcome::Vanished,
            Err(error) => {
                quench_log::warn!("could not read job {object_name}: {error}");
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
}

/// Settles the job, unless a cancel already did.
fn finish(running: &Running, status: Status, exit_code: Option<i32>, error: Option<String>) {
    let mut state = running.state.lock().expect("job state poisoned");
    if state.status.is_terminal() {
        return;
    }

    for step in &mut state.steps {
        match step.status {
            Status::Running if status == Status::Success => {
                step.status = Status::Success;
                step.exit_code = Some(0);
                step.finished_at = Some(Time::now());
            }
            // The step running when the job failed is the one that failed.
            Status::Running => {
                step.status = Status::Failed;
                step.finished_at = Some(Time::now());
            }
            Status::Queued => step.status = Status::Skipped,
            _ => {}
        }
    }

    state.status = status;
    state.exit_code = exit_code;
    state.error = error;
    state.finished_at = Some(Time::now());
}

impl std::fmt::Debug for KubernetesExecutor {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("KubernetesExecutor { .. }")
    }
}
