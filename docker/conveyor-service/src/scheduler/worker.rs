//! The loop that turns a queued run into a finished one.
//!
//! Claim, check out, read the pipeline the commit carries, plan it against this
//! run's context, execute what the plan says, record what happened. Every step
//! of that is written to the database as it goes, so a run that dies halfway
//! leaves a record of how far it got rather than nothing at all.

use crate::artifacts::{self, WarehouseStore};
use crate::config::ConveyorConfig;
use crate::domain::{Repo, Run, Status};
use crate::executors::{JobExecutor, JobSpec, SourceSpec};
use crate::pipeline::{self, Decision, EvalContext, PIPELINE_FILE};
use crate::providers::{CommitStatusReport, Providers};
use crate::scheduler::queue::{self, PlannedJob};
use crate::scheduler::repos;
use crate::secrets::{Redactor, SecretKey, store as secret_store};
use crate::workspace::{self, CheckoutRequest, Workspace};
use quench_db::prelude::Db;
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

/// How long a worker waits before asking for work again when the queue is
/// empty. Short enough that a manual trigger feels immediate, long enough that
/// an idle estate is not running a query several times a second.
const IDLE_POLL: Duration = Duration::from_secs(2);

/// How often a running job's state is read from the executor.
const JOB_POLL: Duration = Duration::from_millis(250);

/// How often, in job polls, the database is asked whether a cancel arrived.
/// Cancellation is rare and the check is a round trip, so it is not worth doing
/// four times a second.
const CANCEL_CHECK_EVERY: u32 = 8;

#[derive(Clone)]
pub struct Worker {
    id: String,
    db: Db,
    config: ConveyorConfig,
    executor: Arc<dyn JobExecutor>,
    providers: Arc<Providers>,
    /// `None` when `CONVEYOR_SECRET_KEY` is unset. A pipeline that declares no
    /// secrets does not need one; one that does fails with a message saying so.
    key: Arc<Option<SecretKey>>,
    /// `None` when no warehouse is configured, in which case declared
    /// artifacts are reported as produced-and-not-kept rather than recorded.
    artifacts: Arc<Option<WarehouseStore>>,
}

/// Starts the worker pool and the janitor, and returns immediately.
///
/// Refuses to start against an in-memory database rather than looking healthy
/// and losing every queued run on restart.
pub fn spawn_pool(
    db: Db,
    config: ConveyorConfig,
    executor: Arc<dyn JobExecutor>,
    providers: Arc<Providers>,
) {
    if let Err(error) = queue::pool(&db) {
        tracing::error!("scheduler not started: {error}");
        return;
    }

    let artifacts = Arc::new(WarehouseStore::from_env());
    if artifacts.is_none() {
        tracing::info!(
            "WAREHOUSE_URL is not set: conveyor will build, but a job's declared \
             artifacts will not be kept anywhere"
        );
    }

    let key = Arc::new(match SecretKey::from_env() {
        Ok(key) => key,
        Err(error) => {
            // Not fatal: pipelines with no secrets build perfectly well. One
            // that needs a secret fails with this same message, where it can
            // actually be seen.
            tracing::error!("secrets are unavailable: {error}");
            None
        }
    });

    let host = envmnt::get_or("HOSTNAME", "conveyor");
    for index in 0..config.max_concurrent_runs {
        let worker = Worker {
            id: format!("{host}-{index}-{}", Uuid::new_v4()),
            db: db.clone(),
            config: config.clone(),
            executor: executor.clone(),
            providers: providers.clone(),
            key: key.clone(),
            artifacts: artifacts.clone(),
        };
        tokio::spawn(worker.run_loop());
    }

    tokio::spawn(janitor(db, config.clone()));

    tracing::info!(
        "scheduler started: {} worker(s), {} executor",
        config.max_concurrent_runs,
        config.executor
    );
}

/// Puts back runs whose worker stopped saying it was alive.
///
/// Without this, one killed worker takes its repository out of service for
/// good: the run stays `running`, and the index that allows only one running
/// run per repository never lets another start.
async fn janitor(db: Db, config: ConveyorConfig) {
    let interval = Duration::from_secs((config.claim_stale_after_secs / 2).max(5));
    loop {
        tokio::time::sleep(interval).await;
        match queue::requeue_stale(&db, config.claim_stale_after_secs).await {
            Ok(0) => {}
            Ok(count) => tracing::warn!("requeued {count} run(s) abandoned by a dead worker"),
            Err(error) => tracing::error!("could not requeue stale runs: {error}"),
        }
    }
}

impl Worker {
    async fn run_loop(self) {
        loop {
            match queue::claim_next(&self.db, &self.id).await {
                Ok(Some(run)) => {
                    let id = run.id.clone();
                    if let Err(error) = self.execute(run).await {
                        // The run is already finished as failed by `execute`
                        // wherever it could be; this is the last resort.
                        tracing::error!("run {id} ended badly: {error}");
                        let _ = queue::finish_run(
                            &self.db,
                            &id,
                            Status::Failed,
                            Some(&error.to_string()),
                        )
                        .await;
                    }
                }
                Ok(None) => tokio::time::sleep(IDLE_POLL).await,
                Err(error) => {
                    tracing::error!("could not claim a run: {error}");
                    tokio::time::sleep(IDLE_POLL).await;
                }
            }
        }
    }

    async fn execute(&self, run: Run) -> Result<(), WorkerError> {
        tracing::info!(
            "run {} claimed: {} at {}",
            run.id,
            run.git_ref,
            run.short_sha()
        );

        // Refreshed while the run works, so the janitor can tell a long build
        // from a dead worker.
        let heartbeat = self.spawn_heartbeat(&run.id);

        // Loaded before the run starts so the pending status can be reported;
        // `perform` takes it rather than reading it again.
        let repo = repos::read(&self.db, &run.repo_id)
            .await?
            .ok_or_else(|| WorkerError::UnknownRepo(run.repo_id.clone()))?;

        self.report(&repo, &run, Status::Running, "build started")
            .await;

        let outcome = self.perform(&run, &repo).await;
        heartbeat.abort();

        let (status, detail) = match outcome {
            Ok((status, detail)) => (status, detail),
            Err(error) => {
                tracing::warn!("run {} failed: {error}", run.id);
                (Status::Failed, Some(error.to_string()))
            }
        };

        queue::finish_run(&self.db, &run.id, status, detail.as_deref()).await?;
        self.report(
            &repo,
            &run,
            status,
            &detail.unwrap_or_else(|| describe(status)),
        )
        .await;

        tracing::info!("run {} finished: {status}", run.id);
        Ok(())
    }

    /// Tells the provider how the commit is doing.
    ///
    /// Never fatal. A repository that builds but cannot be reported on is worth
    /// a warning, not a failed run - and `generic` repositories have nowhere to
    /// report to at all.
    async fn report(&self, repo: &Repo, run: &Run, status: Status, description: &str) {
        let report = CommitStatusReport::new(status, description).with_target(run_url(&run.id));

        match self
            .providers
            .get(repo.provider)
            .report_status(repo, &run.sha, &report)
            .await
        {
            Ok(()) => {}
            // Expected whenever no token is configured, which is a perfectly
            // reasonable way to run conveyor.
            Err(crate::providers::ProviderError::NotConfigured(_)) => {}
            Err(error) => tracing::warn!(
                "could not report {} for {}@{}: {error}",
                status,
                repo.slug(),
                run.short_sha()
            ),
        }
    }

    fn spawn_heartbeat(&self, run_id: &str) -> tokio::task::JoinHandle<()> {
        let db = self.db.clone();
        let worker = self.id.clone();
        let run_id = run_id.to_string();
        let interval = Duration::from_secs((self.config.claim_stale_after_secs / 3).max(5));

        tokio::spawn(async move {
            loop {
                tokio::time::sleep(interval).await;
                if let Err(error) = queue::heartbeat(&db, &run_id, &worker).await {
                    tracing::warn!("heartbeat for run {run_id} failed: {error}");
                }
            }
        })
    }

    /// Everything between the claim and the final status.
    async fn perform(
        &self,
        run: &Run,
        repo: &Repo,
    ) -> Result<(Status, Option<String>), WorkerError> {
        let workspace = self.checkout(run, repo).await?;

        // Read from the checkout: the point of an in-repo pipeline is that the
        // commit supplies it, and the version at HEAD may say something else.
        let source = tokio::fs::read_to_string(workspace.root().join(PIPELINE_FILE))
            .await
            .map_err(|_| WorkerError::NoPipeline)?;
        let spec = pipeline::parse(&source).map_err(|error| WorkerError::BadPipeline {
            reason: error.to_string(),
        })?;

        let event = run.trigger.as_str();
        if !spec.on.allows(event, &run.git_ref) {
            // Not a failure. Registering a repository means conveyor watches
            // it; the pipeline decides which of those events it wants.
            let _ = workspace.remove().await;
            return Ok((
                Status::Skipped,
                Some(format!(
                    "this pipeline does not run on {event} of {}",
                    run.git_ref
                )),
            ));
        }

        let context = EvalContext::new(event, &run.git_ref, &run.sha);
        let plan = pipeline::plan(&spec, &context);

        // The whole plan is written before anything runs, so the run's page can
        // show what it decided not to do as well as what it did.
        let mut planned = Vec::new();
        for stage_plan in &plan {
            let stage = &spec.stages[stage_plan.index];
            for job_plan in &stage_plan.jobs {
                planned.push(PlannedJob {
                    stage: stage.name.clone(),
                    name: stage.jobs[job_plan.index].name.clone(),
                    needs: stage.needs.clone(),
                    status: if job_plan.decision.will_run() {
                        Status::Queued
                    } else {
                        Status::Skipped
                    },
                    error: job_plan.decision.reason(),
                });
            }
        }
        let rows = queue::create_jobs(&self.db, &run.id, &planned).await?;

        let result = self
            .execute_jobs(run, repo, &spec, &plan, &rows, &workspace)
            .await;

        // The checkout goes whatever happened; leaving it would fill the disk
        // one abandoned run at a time.
        if let Err(error) = workspace.remove().await {
            tracing::warn!("could not remove the workspace for run {}: {error}", run.id);
        }

        result
    }

    async fn checkout(&self, run: &Run, repo: &Repo) -> Result<Workspace, WorkerError> {
        workspace::checkout(
            &self.config.work_dir,
            &run.id,
            &CheckoutRequest {
                clone_url: &repo.clone_url,
                git_ref: &run.git_ref,
                sha: &run.sha,
                timeout: Duration::from_secs(self.config.checkout_timeout_secs),
            },
        )
        .await
        .map_err(|error| WorkerError::Checkout(error.to_string()))
    }

    /// Runs the jobs the plan allowed, stage by stage.
    async fn execute_jobs(
        &self,
        run: &Run,
        repo: &Repo,
        spec: &pipeline::PipelineSpec,
        plan: &[pipeline::StagePlan],
        rows: &[crate::domain::Job],
        workspace: &Workspace,
    ) -> Result<(Status, Option<String>), WorkerError> {
        let mut failed_stages: HashSet<String> = HashSet::new();
        let mut statuses = Vec::new();
        let mut cancelled = false;

        for stage_plan in plan {
            let stage = &spec.stages[stage_plan.index];

            // A stage whose dependency failed at run time cannot run, however
            // its own condition read. The plan could not know this: `when` is
            // decided before anything runs, failure is only known afterwards.
            let blocked_by = stage
                .needs
                .iter()
                .find(|need| failed_stages.contains(*need))
                .cloned();

            let mut stage_failed = false;

            for job_plan in &stage_plan.jobs {
                let job = &stage.jobs[job_plan.index];
                let Some(row) = rows
                    .iter()
                    .find(|row| row.stage == stage.name && row.name == job.name)
                else {
                    continue;
                };

                if !job_plan.decision.will_run() {
                    // Already recorded as skipped when the plan was written.
                    statuses.push(Status::Skipped);
                    continue;
                }

                if cancelled || blocked_by.is_some() {
                    let reason = if cancelled {
                        "the run was cancelled".to_string()
                    } else {
                        format!("stage '{}' failed", blocked_by.clone().unwrap_or_default())
                    };
                    queue::finish_job(&self.db, &row.id, Status::Skipped, None, Some(&reason))
                        .await?;
                    statuses.push(Status::Skipped);
                    continue;
                }

                let status = self
                    .run_one_job(run, repo, stage, job, row, workspace)
                    .await?;
                statuses.push(status);

                match status {
                    Status::Cancelled => {
                        cancelled = true;
                        stage_failed = true;
                    }
                    status if status.is_failure() => stage_failed = true,
                    _ => {}
                }
            }

            if stage_failed {
                failed_stages.insert(stage.name.clone());
            }
        }

        if cancelled {
            return Ok((Status::Cancelled, Some("cancelled".to_string())));
        }
        Ok((Status::rollup(statuses), None))
    }

    /// Starts one job, watches it, and records everything it produced.
    async fn run_one_job(
        &self,
        run: &Run,
        repo: &Repo,
        stage: &pipeline::Stage,
        job: &pipeline::Job,
        row: &crate::domain::Job,
        workspace: &Workspace,
    ) -> Result<Status, WorkerError> {
        queue::start_job(&self.db, &row.id).await?;

        // A job sees a secret only if it named one. That is the whole access
        // model: everything else in the store is invisible to it.
        let secrets = match secret_store::resolve(
            &self.db,
            self.key.as_ref().as_ref(),
            &repo.id,
            &job.secrets,
        )
        .await
        {
            Ok(secrets) => secrets,
            Err(error) => {
                // Failing here beats running a deploy step with a blank token,
                // which fails somewhere further on and takes much longer to
                // understand.
                let reason = error.to_string();
                queue::finish_job(&self.db, &row.id, Status::Failed, None, Some(&reason)).await?;
                tracing::warn!("job {} could not get its secrets: {reason}", row.id);
                return Ok(Status::Failed);
            }
        };

        let redactor = Redactor::new(secrets.values().cloned());

        let mut env = self.job_environment(run, repo, stage, job, workspace);
        // Last, so a pipeline cannot shadow a secret with a plain `env` entry
        // of the same name and read what it was given.
        env.extend(secrets);

        let spec = JobSpec {
            id: row.id.clone(),
            name: row.qualified_name(),
            steps: job.steps.clone(),
            env,
            timeout: Duration::from_secs(
                job.timeout.unwrap_or(self.config.default_job_timeout_secs),
            ),
            image: job.image.clone(),
            // Supplied whatever the executor is: the native one runs in the
            // checkout conveyor already made and ignores this, but an executor
            // running off this machine has no other way to get the code.
            source: Some(SourceSpec {
                clone_url: repo.clone_url.clone(),
                git_ref: run.git_ref.clone(),
                sha: run.sha.clone(),
            }),
            redactor,
        };

        let handle = match self.executor.start(&spec, workspace).await {
            Ok(handle) => handle,
            Err(error) => {
                let reason = error.to_string();
                queue::finish_job(&self.db, &row.id, Status::Failed, None, Some(&reason)).await?;
                return Ok(Status::Failed);
            }
        };

        let mut polls: u32 = 0;
        let state = loop {
            let state = self
                .executor
                .poll(&handle)
                .await
                .map_err(|error| WorkerError::Executor(error.to_string()))?;
            if state.is_finished() {
                break state;
            }

            polls = polls.wrapping_add(1);
            if polls.is_multiple_of(CANCEL_CHECK_EVERY)
                && queue::is_cancel_requested(&self.db, &run.id).await?
            {
                tracing::info!("run {} cancelled; stopping {}", run.id, spec.name);
                let _ = self.executor.cancel(&handle).await;
            }

            tokio::time::sleep(JOB_POLL).await;
        };

        // Steps and output first: if writing the job's status fails, the record
        // of what it actually did is already there.
        queue::record_steps(&self.db, &row.id, &state.steps).await?;
        if let Ok(tail) = self.executor.logs(&handle).await {
            queue::append_logs(&self.db, &row.id, &tail.history).await?;
        }
        queue::finish_job(
            &self.db,
            &row.id,
            state.status,
            state.exit_code,
            state.error.as_deref(),
        )
        .await?;

        // Releases the executor's copy of the log, which is now in the database.
        let _ = self.executor.forget(&handle).await;

        // Only for a job that passed. Collecting the output of a failed build
        // would keep whatever half-written thing it left behind.
        if state.status == Status::Success && !job.artifacts.is_empty() {
            self.keep_artifacts(run, row, job, workspace).await;
        }

        Ok(state.status)
    }

    /// Uploads what the job declared, and records where it went.
    ///
    /// Never fails the job. The build passed; losing a copy of its output is
    /// worth a warning rather than turning a green run red.
    async fn keep_artifacts(
        &self,
        run: &Run,
        row: &crate::domain::Job,
        job: &pipeline::Job,
        workspace: &Workspace,
    ) {
        let (kept, problems) = artifacts::collect(
            self.artifacts.as_ref().as_ref(),
            workspace,
            &run.id,
            &row.id,
            &job.artifacts,
        )
        .await;

        for problem in problems {
            tracing::warn!("{}: {problem}", row.qualified_name());
        }

        if self.artifacts.is_none() {
            tracing::warn!(
                "{} produced {} but no warehouse is configured, so {} not kept",
                row.qualified_name(),
                job.artifacts.join(", "),
                if job.artifacts.len() == 1 {
                    "it was"
                } else {
                    "they were"
                }
            );
            return;
        }

        for collected in kept {
            if let Err(error) = queue::record_artifact(&self.db, &collected.artifact).await {
                tracing::error!("could not record an artifact for run {}: {error}", run.id);
            }
        }
    }

    /// What a step sees in its environment.
    ///
    /// `CI` because half the tooling in the world looks for it, and the
    /// `CONVEYOR_*` set so a step can tell what it is building without parsing
    /// anything back out of git.
    fn job_environment(
        &self,
        run: &Run,
        repo: &Repo,
        stage: &pipeline::Stage,
        job: &pipeline::Job,
        workspace: &Workspace,
    ) -> BTreeMap<String, String> {
        let context = EvalContext::new(run.trigger.as_str(), &run.git_ref, &run.sha);

        let mut env = BTreeMap::from([
            ("CI".to_string(), "true".to_string()),
            ("CONVEYOR".to_string(), "true".to_string()),
            ("CONVEYOR_RUN_ID".to_string(), run.id.clone()),
            ("CONVEYOR_REPO".to_string(), repo.slug()),
            ("CONVEYOR_STAGE".to_string(), stage.name.clone()),
            ("CONVEYOR_JOB".to_string(), job.name.clone()),
            ("CONVEYOR_EVENT".to_string(), run.trigger.to_string()),
            ("CONVEYOR_REF".to_string(), run.git_ref.clone()),
            ("CONVEYOR_SHA".to_string(), run.sha.clone()),
            ("CONVEYOR_BRANCH".to_string(), context.branch),
            ("CONVEYOR_TAG".to_string(), context.tag),
            (
                "CONVEYOR_WORKSPACE".to_string(),
                workspace.root().to_string_lossy().to_string(),
            ),
        ]);

        // The pipeline's own values last, so a job can override what conveyor
        // set - which is occasionally what you want and never harmful, since
        // these describe the run rather than grant anything.
        env.extend(job.env.clone());
        env
    }
}

#[derive(Debug, thiserror::Error)]
pub enum WorkerError {
    #[error("repository {0} no longer exists")]
    UnknownRepo(String),

    #[error("checkout failed: {0}")]
    Checkout(String),

    #[error("this commit has no {PIPELINE_FILE}")]
    NoPipeline,

    #[error("{PIPELINE_FILE} is not valid: {reason}")]
    BadPipeline { reason: String },

    #[error("executor: {0}")]
    Executor(String),

    #[error(transparent)]
    Queue(#[from] queue::QueueError),
}

/// Exposed for the tests, which need to know what a decision turns into.
pub fn planned_status(decision: &Decision) -> Status {
    if decision.will_run() {
        Status::Queued
    } else {
        Status::Skipped
    }
}

/// A one-line summary for a provider's status mark.
fn describe(status: Status) -> String {
    match status {
        Status::Success => "all stages passed",
        Status::Failed => "a stage failed",
        Status::Cancelled => "cancelled",
        Status::Skipped => "nothing to build for this event",
        Status::Queued | Status::Running => "building",
    }
    .to_string()
}

/// Where to send someone who clicks the status mark.
///
/// `None` unless the deployment says where it is reachable from - a link to
/// `localhost` on somebody else's machine is worse than no link.
fn run_url(run_id: &str) -> Option<String> {
    let base = envmnt::get_or("CONVEYOR_PUBLIC_URL", "");
    let base = base.trim().trim_end_matches('/');
    (!base.is_empty()).then(|| format!("{base}/ui/runs/{run_id}"))
}
