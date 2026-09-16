//! Where a job's steps actually run - one trait, chosen from config at startup; the scheduler
//! holds an `Arc<dyn JobExecutor>` and doesn't know which it has.

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

/// Newtype so `ContainerBuilder::provide_arc` has a `Sized` type to key by - `dyn JobExecutor` isn't one.
#[derive(Clone)]
pub struct Executor(pub Arc<dyn JobExecutor>);

/// Async because reaching a cluster is: `Client::try_default` can fail in a way worth reporting, not panicking through.
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
            // Never silently falls back to native - that would break the isolation this deployment asked for.
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

/// Stands in for an executor that couldn't be built - every job fails with the reason, rather than falling back to native.
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
