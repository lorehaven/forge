//! Claim, check out, plan, execute, record - every step written to the DB as it goes, so a dead run leaves a trail.

use crate::artifacts::{self, WarehouseStore};
use crate::config::ConveyorConfig;
use crate::credentials::store as credential_store;
use crate::domain::{Repo, Run, Status};
use crate::executors::{JobCredential, JobExecutor, JobSpec, SourceSpec};
use crate::pipeline::{self, Decision, EvalContext, PIPELINE_FILE};
use crate::providers::{CommitStatusReport, Providers};
use crate::scheduler::queue::{self, PlannedJob};
use crate::scheduler::repos;
use crate::secrets::{Redactor, SecretKey, store as secret_store};
use crate::workspace::{self, CheckoutRequest, HttpCredential, Workspace};
use futures_util::StreamExt;
use futures_util::stream::FuturesUnordered;
use quench_db::prelude::Db;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

/// Poll interval when idle - short enough to feel immediate, long enough not to hammer the DB.
const IDLE_POLL: Duration = Duration::from_secs(2);

/// How often a running job's state is read from the executor.
const JOB_POLL: Duration = Duration::from_millis(250);

/// Cancel checks are a DB round trip and rare, so not worth doing every poll.
const CANCEL_CHECK_EVERY: u32 = 8;

#[derive(Clone)]
pub struct Worker {
    id: String,
    db: Db,
    config: ConveyorConfig,
    executor: Arc<dyn JobExecutor>,
    providers: Arc<Providers>,
    /// `None` when `CONVEYOR_SECRET_KEY` is unset - fine unless a pipeline declares secrets.
    key: Arc<Option<SecretKey>>,
    /// `None` when `CONVEYOR_CREDENTIAL_KEY` is unset - fine unless a repo/project needs one.
    credential_key: Arc<Option<SecretKey>>,
    /// `None` when no warehouse is configured - artifacts report as produced-not-kept.
    artifacts: Arc<Option<WarehouseStore>>,
}

/// Starts the worker pool and janitor and returns immediately; refuses an in-memory DB.
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
            // Not fatal: pipelines with no secrets build fine either way.
            tracing::error!("secrets are unavailable: {error}");
            None
        }
    });

    let credential_key = Arc::new(match SecretKey::from_env_named(credential_store::KEY_VAR) {
        Ok(key) => key,
        Err(error) => {
            // Not fatal: a public repo builds fine with no credential key.
            tracing::error!("git credentials are unavailable: {error}");
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
            credential_key: credential_key.clone(),
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

/// Requeues runs whose worker stopped heartbeating - else a dead worker locks its repo forever.
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

/// One job waiting on its stage's outstanding `needs` - `execute_jobs` schedules these, not stages.
struct Unit<'p> {
    stage_index: usize,
    job: &'p pipeline::Job,
    stage: &'p pipeline::Stage,
    row: &'p crate::domain::Job,
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

    /// Tells the provider how the commit is doing - never fatal, just a warning if it can't.
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

        // `Some` even if empty, so a pruned source is distinguishable from "not a restart" below.
        let source_jobs = match &run.resumed_from {
            Some(source_run_id) => match queue::list_jobs(&self.db, source_run_id).await {
                Ok(jobs) => Some(jobs),
                Err(error) => {
                    tracing::warn!(
                        "run {} could not read the run it is restarting ({source_run_id}): \
                         {error}; nothing will be carried over",
                        run.id
                    );
                    None
                }
            },
            None => None,
        };
        let passed_stages = source_jobs
            .as_deref()
            .map(passed_stages)
            .unwrap_or_default();

        // The whole plan is written before anything runs, so the run's page can
        // show what it decided not to do as well as what it did.
        let mut planned = Vec::new();
        for stage_plan in &plan {
            let stage = &spec.stages[stage_plan.index];
            let reused = passed_stages.contains(stage.name.as_str());
            for job_plan in &stage_plan.jobs {
                let (status, error) = if !job_plan.decision.will_run() {
                    (Status::Skipped, job_plan.decision.reason())
                } else if reused {
                    (Status::Success, None)
                } else {
                    (Status::Queued, None)
                };
                planned.push(PlannedJob {
                    stage: stage.name.clone(),
                    name: stage.jobs[job_plan.index].name.clone(),
                    needs: stage.needs.clone(),
                    status,
                    error,
                    reused_from_run: reused.then(|| run.resumed_from.clone().unwrap_or_default()),
                });
            }
        }
        let rows = queue::create_jobs(&self.db, &run.id, &planned).await?;

        if let Some(source_jobs) = &source_jobs {
            self.copy_reused_job_data(run, &rows, source_jobs).await;
        }

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
        let resolved =
            credential_store::resolve(&self.db, self.credential_key.as_ref().as_ref(), repo)
                .await
                .map_err(|error| WorkerError::Checkout(error.to_string()))?;

        let credential = resolved.as_ref().map(|resolved| HttpCredential {
            username: &resolved.username,
            token: &resolved.token,
        });

        // Backstop: a failed clone's git stderr is the one place a token could leak to the run page.
        let redactor = resolved
            .as_ref()
            .map(|resolved| Redactor::new([resolved.token.clone()]))
            .unwrap_or_else(Redactor::none);

        workspace::checkout(
            &self.config.work_dir,
            &run.id,
            &CheckoutRequest {
                clone_url: &repo.clone_url,
                git_ref: &run.git_ref,
                sha: &run.sha,
                timeout: Duration::from_secs(self.config.checkout_timeout_secs),
                credential,
            },
        )
        .await
        .map_err(|error| WorkerError::Checkout(redactor.apply(&error.to_string())))
    }

    /// A job starts the instant its needed stages finish, not in `plan`'s topological order.
    /// Native executor caveat: jobs share one checkout, so racing writes are possible; kubernetes doesn't share this.
    async fn execute_jobs(
        &self,
        run: &Run,
        repo: &Repo,
        spec: &pipeline::PipelineSpec,
        plan: &[pipeline::StagePlan],
        rows: &[crate::domain::Job],
        workspace: &Workspace,
    ) -> Result<(Status, Option<String>), WorkerError> {
        let stage_count = plan.len();

        // A stage can be depended on by several others; reverse edges below find what just became ready.
        let mut index_of: HashMap<&str, usize> = HashMap::with_capacity(stage_count);
        for (index, stage_plan) in plan.iter().enumerate() {
            index_of.insert(spec.stages[stage_plan.index].name.as_str(), index);
        }
        let mut stage_remaining: Vec<usize> = vec![0; stage_count];
        let mut stage_dependents: Vec<Vec<usize>> = vec![Vec::new(); stage_count];
        for (index, stage_plan) in plan.iter().enumerate() {
            for need in &spec.stages[stage_plan.index].needs {
                if let Some(&needed) = index_of.get(need.as_str()) {
                    stage_dependents[needed].push(index);
                    stage_remaining[index] += 1;
                }
            }
        }

        // Every job flattened from its stage, so a stage finishing can check all its jobs did too.
        let mut units: Vec<Unit> = Vec::new();
        let mut units_of_stage: Vec<Vec<usize>> = vec![Vec::new(); stage_count];
        for (stage_index, stage_plan) in plan.iter().enumerate() {
            let stage = &spec.stages[stage_plan.index];
            for job_plan in &stage_plan.jobs {
                let job = &stage.jobs[job_plan.index];
                let Some(row) = rows
                    .iter()
                    .find(|row| row.stage == stage.name && row.name == job.name)
                else {
                    continue;
                };
                units_of_stage[stage_index].push(units.len());
                units.push(Unit {
                    stage_index,
                    job,
                    stage,
                    row,
                });
            }
        }
        let unit_count = units.len();

        let mut ready: VecDeque<usize> = VecDeque::new();
        for stage_index in 0..stage_count {
            if stage_remaining[stage_index] == 0 {
                ready.extend(units_of_stage[stage_index].iter().copied());
            }
        }

        let mut failed_stages: HashSet<String> = HashSet::new();
        let mut cancelled = false;
        let mut all_statuses = Vec::new();
        let mut stage_finished: Vec<usize> = vec![0; stage_count];
        let mut finished_units = 0usize;
        let mut running = FuturesUnordered::new();

        while finished_units < unit_count {
            // Readiness already proves `needs` is done, so the current `failed_stages`/`cancelled` suffice.
            while let Some(unit_index) = ready.pop_front() {
                let Unit {
                    stage_index,
                    job,
                    stage,
                    row,
                } = &units[unit_index];
                let blocked_by = stage
                    .needs
                    .iter()
                    .find(|need| failed_stages.contains(*need))
                    .cloned();
                let cancelled_now = cancelled;
                let stage_index = *stage_index;

                running.push(async move {
                    // Already settled at plan time: excluded by `when`, or carried over from a restart.
                    let status = if row.status != Status::Queued {
                        row.status
                    } else if cancelled_now || blocked_by.is_some() {
                        let reason = if cancelled_now {
                            "the run was cancelled".to_string()
                        } else {
                            format!("stage '{}' failed", blocked_by.clone().unwrap_or_default())
                        };
                        queue::finish_job(&self.db, &row.id, Status::Skipped, None, Some(&reason))
                            .await?;
                        Status::Skipped
                    } else {
                        self.run_one_job(run, repo, stage, job, row, workspace)
                            .await?
                    };

                    Ok::<_, WorkerError>((stage_index, status))
                });
            }

            let Some(outcome) = running.next().await else {
                // Only reached once `ready` and `running` are both empty.
                break;
            };
            let (stage_index, status) = outcome?;
            finished_units += 1;
            all_statuses.push(status);

            if status == Status::Cancelled {
                cancelled = true;
            }
            if status.is_failure() {
                failed_stages.insert(spec.stages[plan[stage_index].index].name.clone());
            }

            stage_finished[stage_index] += 1;
            if stage_finished[stage_index] < units_of_stage[stage_index].len() {
                // Dependents wait for the whole stage, not just this one job.
                continue;
            }
            for &dependent in &stage_dependents[stage_index] {
                stage_remaining[dependent] -= 1;
                if stage_remaining[dependent] == 0 {
                    ready.extend(units_of_stage[dependent].iter().copied());
                }
            }
        }

        if cancelled {
            return Ok((Status::Cancelled, Some("cancelled".to_string())));
        }
        Ok((Status::rollup(all_statuses), None))
    }

    /// Copies a reused job's steps/log/artifacts so a restart reads as one coherent record.
    /// Never fails the restart itself - a copy error just leaves that job without a log.
    async fn copy_reused_job_data(
        &self,
        run: &Run,
        rows: &[crate::domain::Job],
        source_jobs: &[crate::domain::Job],
    ) {
        for row in rows {
            if row.reused_from_run.is_none() {
                continue;
            }

            let Some(source_job) = source_jobs
                .iter()
                .find(|job| job.stage == row.stage && job.name == row.name)
            else {
                tracing::warn!(
                    "run {} reused stage '{}', but its source job for '{}' is gone; \
                     {} will show no log",
                    run.id,
                    row.stage,
                    row.name,
                    row.id
                );
                continue;
            };

            match queue::list_steps(&self.db, &source_job.id).await {
                Ok(steps) => {
                    if let Err(error) = queue::record_steps(&self.db, &row.id, &steps).await {
                        tracing::warn!(
                            "could not copy steps from job {} to {}: {error}",
                            source_job.id,
                            row.id
                        );
                    }
                }
                Err(error) => {
                    tracing::warn!("could not read steps for job {}: {error}", source_job.id)
                }
            }

            match queue::read_logs(&self.db, &source_job.id, -1).await {
                Ok(chunks) => {
                    if let Err(error) = queue::append_logs(&self.db, &row.id, &chunks).await {
                        tracing::warn!(
                            "could not copy the log from job {} to {}: {error}",
                            source_job.id,
                            row.id
                        );
                    }
                }
                Err(error) => {
                    tracing::warn!("could not read the log for job {}: {error}", source_job.id)
                }
            }

            match queue::list_artifacts_for_job(&self.db, &source_job.id).await {
                Ok(artifacts) => {
                    for artifact in artifacts {
                        let copy = crate::domain::Artifact {
                            id: Uuid::new_v4().to_string(),
                            run_id: run.id.clone(),
                            job_id: row.id.clone(),
                            kind: artifact.kind,
                            name: artifact.name,
                            version: artifact.version,
                            uri: artifact.uri,
                            digest: artifact.digest,
                            created_at: artifact.created_at,
                        };
                        if let Err(error) = queue::record_artifact(&self.db, &copy).await {
                            tracing::warn!(
                                "could not copy an artifact from job {} to {}: {error}",
                                source_job.id,
                                row.id
                            );
                        }
                    }
                }
                Err(error) => tracing::warn!(
                    "could not read artifacts for job {}: {error}",
                    source_job.id
                ),
            }

            if let Err(error) = queue::finish_job(
                &self.db,
                &row.id,
                Status::Success,
                source_job.exit_code,
                None,
            )
            .await
            {
                tracing::warn!("could not close out reused job {}: {error}", row.id);
            }
        }
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
                // Beats running a deploy step with a blank token that fails confusingly later.
                let reason = error.to_string();
                queue::finish_job(&self.db, &row.id, Status::Failed, None, Some(&reason)).await?;
                tracing::warn!("job {} could not get its secrets: {reason}", row.id);
                return Ok(Status::Failed);
            }
        };

        // Same credential `checkout()` uses - an off-machine executor's init container needs its own copy.
        let credential =
            credential_store::resolve(&self.db, self.credential_key.as_ref().as_ref(), repo)
                .await
                .unwrap_or_else(|error| {
                    tracing::warn!("could not resolve a credential for {}: {error}", repo.id);
                    None
                });

        let mut redacted = secrets.values().cloned().collect::<Vec<_>>();
        if let Some(credential) = &credential {
            redacted.push(credential.token.clone());
        }
        let redactor = Redactor::new(redacted);

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
            // Native executor ignores this (already has the checkout); an off-machine one needs it.
            source: Some(SourceSpec {
                clone_url: repo.clone_url.clone(),
                git_ref: run.git_ref.clone(),
                sha: run.sha.clone(),
                credential: credential.map(|c| JobCredential {
                    username: c.username,
                    token: c.token,
                }),
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

    /// Uploads declared artifacts; never fails the job - a lost copy is a warning, not a red run.
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

    /// What a step sees in its environment - `CI` for common tooling, `CONVEYOR_*` for context.
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

        // Pipeline values last, so a job can override conveyor's - harmless, these just describe the run.
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

/// Stages a restart can skip: every one whose jobs all ended `Success` last time.
fn passed_stages(jobs: &[crate::domain::Job]) -> HashSet<String> {
    let mut ok: HashMap<&str, bool> = HashMap::new();
    for job in jobs {
        let entry = ok.entry(job.stage.as_str()).or_insert(true);
        *entry = *entry && job.status == Status::Success;
    }
    ok.into_iter()
        .filter(|(_, passed)| *passed)
        .map(|(stage, _)| stage.to_string())
        .collect()
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

/// `None` unless the deployment says where it's reachable - a `localhost` link is worse than none.
fn run_url(run_id: &str) -> Option<String> {
    let base = envmnt::get_or("CONVEYOR_PUBLIC_URL", "");
    let base = base.trim().trim_end_matches('/');
    (!base.is_empty()).then(|| format!("{base}/ui/runs/{run_id}"))
}
