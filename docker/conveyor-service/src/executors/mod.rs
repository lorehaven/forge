//! Where a job's steps actually run.
//!
//! One trait, one implementation per place the work can happen, chosen from
//! configuration at startup. The scheduler holds an `Arc<dyn JobExecutor>` and
//! does not know which it has.

use crate::config::ExecutorKind;
use std::sync::Arc;

pub mod engine;
pub mod kubernetes;
pub mod manifest;
pub mod mock;
pub mod native;

pub use engine::{
    ExecError, Handle, JobCredential, JobExecutor, JobSpec, JobState, LogChunk, LogTail,
    SourceSpec, StepState, Stream,
};
pub use kubernetes::KubernetesExecutor;
pub use mock::{MockExecutor, MockOutcome};
pub use native::NativeExecutor;

/// Builds the executor a deployment asked for.
///
/// Async because reaching a cluster is: `Client::try_default` reads a
/// kubeconfig or the pod's service account, and either can fail in a way worth
/// reporting rather than panicking through.
pub async fn build(kind: ExecutorKind) -> Arc<dyn JobExecutor> {
    match kind {
        ExecutorKind::Native => Arc::new(NativeExecutor::new()),
        ExecutorKind::Mock => {
            tracing::warn!(
                "CONVEYOR_EXECUTOR=mock: jobs will report success without running anything"
            );
            Arc::new(MockExecutor::new())
        }
        ExecutorKind::Kubernetes => match KubernetesExecutor::connect().await {
            Ok(executor) => Arc::new(executor),
            // Deliberately not a silent fall back to native. This deployment
            // asked for isolation, and running a repository's pipeline inside
            // conveyor's own container instead is the one substitution that
            // must never happen quietly.
            Err(error) => {
                tracing::error!(
                    "CONVEYOR_EXECUTOR=kubernetes but the cluster is unreachable: {error}. \
                     Refusing to run pipelines inside this container instead; every job \
                     will fail until it is fixed."
                );
                Arc::new(UnavailableExecutor {
                    reason: error.to_string(),
                })
            }
        },
    }
}

/// Stands in for an executor that could not be built.
///
/// Every job fails, with the reason. The alternative - falling back to the
/// native executor - would run a repository's pipeline with this service's
/// privileges on a deployment that explicitly asked for it not to.
struct UnavailableExecutor {
    reason: String,
}

#[async_trait::async_trait]
impl JobExecutor for UnavailableExecutor {
    fn name(&self) -> &'static str {
        "unavailable"
    }

    async fn start(
        &self,
        _spec: &JobSpec,
        _workspace: &crate::workspace::Workspace,
    ) -> Result<Handle, ExecError> {
        Err(ExecError::Unsupported {
            executor: "kubernetes",
            what: format!("run anything: {}", self.reason),
        })
    }

    async fn poll(&self, handle: &Handle) -> Result<JobState, ExecError> {
        Err(ExecError::UnknownHandle(handle.clone()))
    }

    async fn logs(&self, handle: &Handle) -> Result<LogTail, ExecError> {
        Err(ExecError::UnknownHandle(handle.clone()))
    }

    async fn cancel(&self, _handle: &Handle) -> Result<(), ExecError> {
        Ok(())
    }

    async fn forget(&self, _handle: &Handle) -> Result<(), ExecError> {
        Ok(())
    }
}
