//! How an operation's runner is started and watched. Two implementations behind one trait: a Kubernetes Job
//! (the real thing, and the only kind that survives Gantry restarting itself) and a local child process
//! (so the whole service can be tried on a laptop before it goes near a cluster).

use crate::domain::cluster::{Cluster, ClusterError, JobSpec, JobStatus};
use crate::domain::settings::{RunnerMode, Settings};
use crate::domain::steps::Plan;
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct Launch {
    pub operation_id: String,
    /// Deterministic from the operation id; see `operation::job_name`.
    pub name: String,
    pub plan: Plan,
}

#[async_trait]
pub trait Executor: Send + Sync {
    /// Idempotent: starting a runner that already exists leaves it alone.
    async fn start(&self, launch: &Launch) -> Result<(), ClusterError>;
    async fn status(&self, name: &str) -> Result<JobStatus, ClusterError>;
    async fn log(&self, name: &str) -> Result<String, ClusterError>;
    /// Stops it and clears it away.
    async fn stop(&self, name: &str) -> Result<(), ClusterError>;
    /// Tidies up after a finished runner, once its log is stored.
    async fn cleanup(&self, name: &str) -> Result<(), ClusterError>;
}

pub fn build(
    settings: &Settings,
    cluster: Arc<dyn Cluster>,
    fake: Option<Arc<crate::domain::cluster::FakeCluster>>,
    registry: Arc<dyn crate::domain::registry::Registry>,
) -> Arc<dyn Executor> {
    match settings.runner {
        RunnerMode::Simulate => match fake {
            Some(fake) => Arc::new(crate::domain::simulate::SimulatedExecutor::new(
                fake, registry,
            )),
            // Settings refuse this combination; stay inert rather than pretend.
            None => Arc::new(LocalExecutor::new(settings.clone(), true)),
        },
        RunnerMode::Job => Arc::new(ClusterExecutor {
            cluster,
            settings: settings.clone(),
        }),
        RunnerMode::Local => Arc::new(LocalExecutor::new(settings.clone(), false)),
        RunnerMode::DryRun => Arc::new(LocalExecutor::new(settings.clone(), true)),
    }
}

// ---------------------------------------------------------------- job

pub struct ClusterExecutor {
    pub cluster: Arc<dyn Cluster>,
    pub settings: Settings,
}

impl ClusterExecutor {
    pub fn spec(&self, launch: &Launch) -> Result<JobSpec, ClusterError> {
        Ok(JobSpec {
            namespace: self.settings.namespace.clone(),
            name: launch.name.clone(),
            image: self.settings.runner_image.clone(),
            service_account: self.settings.runner_service_account.clone(),
            plan_json: serde_json::to_string(&launch.plan)
                .map_err(|e| ClusterError::Kube(e.to_string()))?,
            allowed_namespaces: self.settings.allowed_namespaces.clone(),
            credentials_secret: self.settings.credentials_secret.clone(),
            age_key_secret: self.settings.age_key_secret.clone(),
            values_secrets: launch.plan.values_secrets(),
            operation_id: launch.operation_id.clone(),
            deadline_secs: self.settings.job_deadline_secs,
        })
    }
}

#[async_trait]
impl Executor for ClusterExecutor {
    async fn start(&self, launch: &Launch) -> Result<(), ClusterError> {
        self.cluster.create_job(&self.spec(launch)?).await
    }

    async fn status(&self, name: &str) -> Result<JobStatus, ClusterError> {
        self.cluster
            .job_status(&self.settings.namespace, name)
            .await
    }

    async fn log(&self, name: &str) -> Result<String, ClusterError> {
        self.cluster.job_log(&self.settings.namespace, name).await
    }

    async fn stop(&self, name: &str) -> Result<(), ClusterError> {
        self.cluster
            .delete_job(&self.settings.namespace, name)
            .await
    }

    async fn cleanup(&self, name: &str) -> Result<(), ClusterError> {
        self.cluster
            .delete_job(&self.settings.namespace, name)
            .await
    }
}

// ---------------------------------------------------------------- local

/// A runner as a child process. Files in the state directory are its whole state, so the service can
/// restart and find a runner it started, the same way it finds a Job:
/// `<name>.pid`, `<name>.log` and `<name>.exit` (written last, by the runner's own shell).
pub struct LocalExecutor {
    settings: Settings,
    dry_run: bool,
}

impl LocalExecutor {
    pub fn new(settings: Settings, dry_run: bool) -> Self {
        Self { settings, dry_run }
    }

    fn file(&self, name: &str, extension: &str) -> PathBuf {
        self.settings.state_dir.join(format!("{name}.{extension}"))
    }
}

fn io(error: &std::io::Error) -> ClusterError {
    ClusterError::Kube(error.to_string())
}

#[async_trait]
impl Executor for LocalExecutor {
    async fn start(&self, launch: &Launch) -> Result<(), ClusterError> {
        tokio::fs::create_dir_all(&self.settings.state_dir)
            .await
            .map_err(|e| io(&e))?;
        // Already started (by this process or a previous one): leave it.
        if tokio::fs::try_exists(self.file(&launch.name, "pid"))
            .await
            .unwrap_or(false)
        {
            return Ok(());
        }

        let plan =
            serde_json::to_string(&launch.plan).map_err(|e| ClusterError::Kube(e.to_string()))?;
        let script =
            r#""$GANTRY_RUNNER_BIN" > "$GANTRY_LOG_FILE" 2>&1; echo $? > "$GANTRY_EXIT_FILE""#;

        let mut child = tokio::process::Command::new("sh")
            .args(["-c", script])
            .env("GANTRY_PLAN", plan)
            .env("GANTRY_RUNNER_BIN", &self.settings.runner_bin)
            .env("GANTRY_LOG_FILE", self.file(&launch.name, "log"))
            .env("GANTRY_EXIT_FILE", self.file(&launch.name, "exit"))
            .env("GANTRY_VALUES_DIR", &self.settings.values_dir)
            .env(
                "GANTRY_PACKAGES_DIR",
                self.settings
                    .state_dir
                    .join(format!("{}-packages", launch.name)),
            )
            .env(
                "GANTRY_NAMESPACES",
                self.settings.allowed_namespaces.join(","),
            )
            .env("GANTRY_DRY_RUN", if self.dry_run { "1" } else { "0" })
            .env(
                "GANTRY_SOURCE_PACKAGES_DIR",
                self.settings
                    .packages_dir
                    .as_deref()
                    .map(std::path::Path::as_os_str)
                    .unwrap_or_default(),
            )
            // Its own process group, so stopping it stops the runner and what the runner started.
            .process_group(0)
            .spawn()
            .map_err(|e| io(&e))?;
        // Written by us, straight after the spawn, so "started" is visible the moment `start` returns.
        if let Some(pid) = child.id() {
            tokio::fs::write(self.file(&launch.name, "pid"), pid.to_string())
                .await
                .map_err(|e| io(&e))?;
        }
        tokio::spawn(async move {
            let _ = child.wait().await;
        });
        Ok(())
    }

    async fn status(&self, name: &str) -> Result<JobStatus, ClusterError> {
        let exited = |code: String| match code.trim() {
            "0" => JobStatus::Succeeded,
            code => JobStatus::Failed(format!("the runner exited with {code}")),
        };

        if let Ok(code) = tokio::fs::read_to_string(self.file(name, "exit")).await {
            return Ok(exited(code));
        }
        let Ok(pid) = tokio::fs::read_to_string(self.file(name, "pid")).await else {
            return Ok(JobStatus::Absent);
        };
        if tokio::fs::try_exists(format!("/proc/{}", pid.trim()))
            .await
            .unwrap_or(false)
        {
            return Ok(JobStatus::Active);
        }
        // Dead - but it may have written its exit code between the two looks above.
        if let Ok(code) = tokio::fs::read_to_string(self.file(name, "exit")).await {
            return Ok(exited(code));
        }
        Ok(JobStatus::Failed(
            "the runner process disappeared without finishing".to_string(),
        ))
    }

    async fn log(&self, name: &str) -> Result<String, ClusterError> {
        Ok(tokio::fs::read_to_string(self.file(name, "log"))
            .await
            .unwrap_or_default())
    }

    async fn stop(&self, name: &str) -> Result<(), ClusterError> {
        if let Ok(pid) = tokio::fs::read_to_string(self.file(name, "pid")).await {
            let _ = tokio::process::Command::new("kill")
                .args(["-TERM", "--", &format!("-{}", pid.trim())])
                .status()
                .await;
        }
        Ok(())
    }

    async fn cleanup(&self, _name: &str) -> Result<(), ClusterError> {
        // The files are the history of a local run; leave them for the developer to read.
        Ok(())
    }
}
