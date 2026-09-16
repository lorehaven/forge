//! Conveyor's knobs, read once at startup - environment-driven like the rest of the estate.

use std::fmt;
use std::path::PathBuf;

/// Where a job's steps actually run.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ExecutorKind {
    /// Child processes of this service, in a checkout on local disk.
    #[default]
    Native,
    /// One `batch/v1` Job per conveyor job.
    Kubernetes,
    /// Tests only - a real deployment builds nothing with this selected.
    Mock,
}

impl ExecutorKind {
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "kubernetes" | "k8s" => Self::Kubernetes,
            "mock" => Self::Mock,
            "native" | "" => Self::Native,
            other => {
                tracing::warn!("unknown CONVEYOR_EXECUTOR {other:?}, falling back to native");
                Self::Native
            }
        }
    }
}

impl fmt::Display for ExecutorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Native => "native",
            Self::Kubernetes => "kubernetes",
            Self::Mock => "mock",
        })
    }
}

#[derive(Clone, Debug)]
pub struct ConveyorConfig {
    pub executor: ExecutorKind,

    /// Root for per-run checkouts; each run's directory is removed when it finishes.
    pub work_dir: PathBuf,

    /// Runs this replica executes at once; per-repo serialisation is separate.
    pub max_concurrent_runs: usize,

    /// Ceiling on a single job, applied when the pipeline does not set its own.
    pub default_job_timeout_secs: u64,

    /// Ceiling on the checkout, so a hung fetch doesn't hold a worker forever.
    pub checkout_timeout_secs: u64,

    /// A run whose worker stopped reporting this long is considered lost and requeued.
    pub claim_stale_after_secs: u64,

    /// Off by default, and should stay off under native: a fork's `.conveyor.toml` would run with this service's privileges.
    pub allow_fork_pr: bool,

    /// How many runs the front page's pipeline panel shows at once.
    pub home_recent_runs: usize,

    /// Cap per repository, so one noisy repo can't push every other one off the front page.
    pub home_max_runs_per_repo: usize,

    /// How many runs a page of the full pipeline history shows.
    pub runs_page_size: usize,
}

impl Default for ConveyorConfig {
    fn default() -> Self {
        Self {
            executor: ExecutorKind::default(),
            work_dir: PathBuf::from("/tmp/conveyor"),
            max_concurrent_runs: 2,
            default_job_timeout_secs: 3600,
            checkout_timeout_secs: 600,
            claim_stale_after_secs: 300,
            allow_fork_pr: false,
            home_recent_runs: 5,
            home_max_runs_per_repo: 1,
            runs_page_size: 25,
        }
    }
}

impl ConveyorConfig {
    pub fn load() -> Self {
        let defaults = Self::default();

        let config = Self {
            executor: ExecutorKind::parse(&envmnt::get_or("CONVEYOR_EXECUTOR", "native")),
            work_dir: PathBuf::from(envmnt::get_or(
                "CONVEYOR_WORK_DIR",
                &defaults.work_dir.to_string_lossy(),
            )),
            max_concurrent_runs: positive(
                "CONVEYOR_MAX_CONCURRENT_RUNS",
                defaults.max_concurrent_runs,
            ),
            default_job_timeout_secs: positive(
                "CONVEYOR_JOB_TIMEOUT_SECS",
                defaults.default_job_timeout_secs as usize,
            ) as u64,
            checkout_timeout_secs: positive(
                "CONVEYOR_CHECKOUT_TIMEOUT_SECS",
                defaults.checkout_timeout_secs as usize,
            ) as u64,
            claim_stale_after_secs: positive(
                "CONVEYOR_CLAIM_STALE_AFTER_SECS",
                defaults.claim_stale_after_secs as usize,
            ) as u64,
            allow_fork_pr: envmnt::is_or("CONVEYOR_ALLOW_FORK_PR", false),
            home_recent_runs: positive("CONVEYOR_HOME_RECENT_RUNS", defaults.home_recent_runs),
            home_max_runs_per_repo: positive(
                "CONVEYOR_HOME_MAX_RUNS_PER_REPO",
                defaults.home_max_runs_per_repo,
            ),
            runs_page_size: positive("CONVEYOR_RUNS_PAGE_SIZE", defaults.runs_page_size),
        };

        if config.allow_fork_pr && config.executor == ExecutorKind::Native {
            tracing::warn!(
                "CONVEYOR_ALLOW_FORK_PR is on under the native executor: a pipeline \
                 defined in someone else's fork will run with this service's \
                 privileges, including its database and secret key"
            );
        }

        config
    }
}

/// Reads a positive integer, defaulting on missing/unparseable/zero (zero silently does nothing, so it's treated as garbage too).
pub fn positive(key: &str, default: usize) -> usize {
    let raw = envmnt::get_or(key, "");
    if raw.trim().is_empty() {
        return default;
    }

    match raw.trim().parse::<usize>() {
        Ok(value) if value > 0 => value,
        _ => {
            tracing::warn!("{key}={raw:?} is not a positive integer, using {default}");
            default
        }
    }
}
