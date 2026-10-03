//! The pieces the handlers and the reconciler share, in one injectable value.

use crate::domain::GantryError;
use crate::domain::cluster::Cluster;
use crate::domain::executor::Executor;
use crate::domain::operation::{CLUSTER_SCOPE, NewOperation, Operation, Store};
use crate::domain::registry::Registry;
use crate::domain::settings::Settings;
use crate::domain::steps::Plan;
use std::sync::Arc;
use tokio::sync::Notify;

pub struct Gantry {
    pub store: Arc<dyn Store>,
    pub cluster: Arc<dyn Cluster>,
    pub executor: Arc<dyn Executor>,
    pub registry: Arc<dyn Registry>,
    pub settings: Settings,
    wake: Notify,
}

#[derive(Debug, thiserror::Error)]
pub enum SubmitError {
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Store(#[from] GantryError),
}

impl Gantry {
    pub fn new(
        store: Arc<dyn Store>,
        cluster: Arc<dyn Cluster>,
        executor: Arc<dyn Executor>,
        registry: Arc<dyn Registry>,
        settings: Settings,
    ) -> Self {
        Self {
            store,
            cluster,
            executor,
            registry,
            settings,
            wake: Notify::new(),
        }
    }

    /// Records an operation (validated first: a plan outside the allow-list is refused, not queued) and
    /// nudges the reconciler. Nothing is done here - the row is the commitment, the reconciler does the work.
    pub async fn submit(
        &self,
        kind: &str,
        title: &str,
        plan: Plan,
        requested_by: &str,
    ) -> Result<Operation, SubmitError> {
        if title.trim().is_empty() {
            return Err(SubmitError::Invalid("a title is required".to_string()));
        }
        plan.validate(&self.settings.allowed_namespaces)
            .map_err(SubmitError::Invalid)?;

        // Whatever asked for it - the planner, or a raw plan posted to the API - nothing may take away
        // what Gantry stands on. Upgrading these is fine; stopping them is not.
        for step in &plan.steps {
            for (namespace, name) in step.stops() {
                if self
                    .settings
                    .protected
                    .iter()
                    .any(|(ns, n)| ns == namespace && n == name)
                {
                    return Err(SubmitError::Invalid(format!(
                        "{namespace}/{name} is protected and cannot be stopped: Gantry, its database and \
                         its login depend on it"
                    )));
                }
            }
        }

        let operation = self
            .store
            .create(&NewOperation {
                kind: kind.to_string(),
                title: title.trim().to_string(),
                scope: CLUSTER_SCOPE.to_string(),
                plan,
                requested_by: requested_by.to_string(),
            })
            .await?;
        self.wake.notify_one();
        Ok(operation)
    }

    pub async fn cancel(&self, id: &str) -> Result<Option<Operation>, GantryError> {
        let operation = self.store.request_cancel(id).await?;
        self.wake.notify_one();
        Ok(operation)
    }

    /// Resolves when something asks for a reconcile.
    pub async fn woken(&self) {
        self.wake.notified().await;
    }
}
