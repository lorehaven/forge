//! Operations: one row per requested change, written before anything is done, so a restart finds where
//! things stood. `Store` is the seam: Postgres in the service, memory in tests and in a local run.

use crate::domain::steps::Plan;
use crate::domain::{GantryError, pool, schema};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use quench_db::prelude::Db;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::collections::BTreeMap;
use std::sync::Mutex;
use uuid::Uuid;

/// Operations default to one scope: everything serialises, which is what stops two upgrades racing.
pub const CLUSTER_SCOPE: &str = "cluster";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Queued,
    Running,
    Succeeded,
    Failed,
    RolledBack,
    Cancelled,
}

impl State {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::RolledBack => "rolled_back",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "queued" => Self::Queued,
            "running" => Self::Running,
            "succeeded" => Self::Succeeded,
            "failed" => Self::Failed,
            "rolled_back" => Self::RolledBack,
            "cancelled" => Self::Cancelled,
            _ => return None,
        })
    }

    pub fn is_terminal(self) -> bool {
        !matches!(self, Self::Queued | Self::Running)
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Operation {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub scope: String,
    pub state: State,
    pub plan: Plan,
    pub result: Option<serde_json::Value>,
    pub runner_job: Option<String>,
    pub requested_by: String,
    pub cancel_requested: bool,
    pub error: Option<String>,
    pub log: Option<String>,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug)]
pub struct NewOperation {
    pub kind: String,
    pub title: String,
    pub scope: String,
    pub plan: Plan,
    pub requested_by: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct StoredPlan {
    pub id: String,
    pub target: String,
    pub action: String,
    pub version: Option<String>,
    /// The version that was running when the plan was made.
    pub basis: Option<String>,
    pub plan: Plan,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    /// Set once confirmed: a plan runs at most once.
    pub operation_id: Option<String>,
}

#[derive(Clone, Debug)]
pub struct NewPlan {
    pub target: String,
    pub action: String,
    pub version: Option<String>,
    pub basis: Option<String>,
    pub plan: Plan,
    pub created_by: String,
}

/// What a package declared the last time it was installed successfully.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Inventory {
    pub version: String,
    pub resources: Vec<crate::domain::resources::InventoryItem>,
}

/// The runner Job's name, derived from the operation id: the same id always names the same Job, which is
/// how a restarted service finds the Job a previous incarnation started.
pub fn job_name(operation_id: &str) -> String {
    let short: String = operation_id
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(12)
        .collect::<String>()
        .to_lowercase();
    format!("gantry-op-{short}")
}

#[async_trait]
pub trait Store: Send + Sync {
    async fn create(&self, new: &NewOperation) -> Result<Operation, GantryError>;
    async fn get(&self, id: &str) -> Result<Option<Operation>, GantryError>;
    /// Newest first.
    async fn list(&self, limit: i64) -> Result<Vec<Operation>, GantryError>;
    async fn running(&self) -> Result<Vec<Operation>, GantryError>;
    /// Moves the oldest queued operation whose scope has nothing running to `running` and returns it.
    /// The one-per-scope rule is enforced by the store (the database's unique index), not by the caller.
    async fn claim_next(&self) -> Result<Option<Operation>, GantryError>;
    /// A queued operation is cancelled at once; a running one is flagged, and the reconciler stops its Job.
    async fn request_cancel(&self, id: &str) -> Result<Option<Operation>, GantryError>;
    async fn finish(
        &self,
        id: &str,
        state: State,
        error: Option<&str>,
        log: Option<&str>,
        result: Option<serde_json::Value>,
    ) -> Result<(), GantryError>;

    async fn all_inventories(&self) -> Result<BTreeMap<String, Inventory>, GantryError>;
    async fn set_inventory(&self, package: &str, inventory: &Inventory) -> Result<(), GantryError>;

    /// Deployment name -> `running` / `stopped`, for the ones someone has started or stopped.
    async fn deployment_states(&self) -> Result<BTreeMap<String, String>, GantryError>;
    async fn set_deployment_state(
        &self,
        deployment: &str,
        desired: &str,
        by: &str,
    ) -> Result<(), GantryError>;

    async fn save_plan(&self, new: &NewPlan) -> Result<StoredPlan, GantryError>;
    async fn plan(&self, id: &str) -> Result<Option<StoredPlan>, GantryError>;
    /// Marks a plan as being confirmed, once. `false` if it already was.
    async fn claim_plan(&self, id: &str) -> Result<bool, GantryError>;
    /// Records the operation a claimed plan became, or releases the claim (`None`) if submitting failed.
    async fn settle_plan(&self, id: &str, operation_id: Option<&str>) -> Result<(), GantryError>;
}

/// While a plan is being turned into an operation.
pub const CLAIMING: &str = "claiming";

// ---------------------------------------------------------------- memory

#[derive(Default)]
pub struct MemoryStore {
    rows: Mutex<Vec<Operation>>,
    plans: Mutex<Vec<StoredPlan>>,
    states: Mutex<BTreeMap<String, String>>,
    inventories: Mutex<BTreeMap<String, Inventory>>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }

    fn rows(&self) -> std::sync::MutexGuard<'_, Vec<Operation>> {
        self.rows
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[async_trait]
impl Store for MemoryStore {
    async fn create(&self, new: &NewOperation) -> Result<Operation, GantryError> {
        let operation = Operation {
            id: Uuid::new_v4().to_string(),
            kind: new.kind.clone(),
            title: new.title.clone(),
            scope: new.scope.clone(),
            state: State::Queued,
            plan: new.plan.clone(),
            result: None,
            runner_job: None,
            requested_by: new.requested_by.clone(),
            cancel_requested: false,
            error: None,
            log: None,
            created_at: Utc::now(),
            started_at: None,
            finished_at: None,
        };
        self.rows().push(operation.clone());
        Ok(operation)
    }

    async fn get(&self, id: &str) -> Result<Option<Operation>, GantryError> {
        Ok(self.rows().iter().find(|row| row.id == id).cloned())
    }

    async fn list(&self, limit: i64) -> Result<Vec<Operation>, GantryError> {
        let mut rows = self.rows().clone();
        rows.sort_by_key(|row| std::cmp::Reverse(row.created_at));
        rows.truncate(usize::try_from(limit).unwrap_or(0));
        Ok(rows)
    }

    async fn running(&self) -> Result<Vec<Operation>, GantryError> {
        Ok(self
            .rows()
            .iter()
            .filter(|row| row.state == State::Running)
            .cloned()
            .collect())
    }

    async fn claim_next(&self) -> Result<Option<Operation>, GantryError> {
        let mut rows = self.rows();
        let busy: Vec<String> = rows
            .iter()
            .filter(|row| row.state == State::Running)
            .map(|row| row.scope.clone())
            .collect();
        let next = rows
            .iter_mut()
            .filter(|row| row.state == State::Queued && !busy.contains(&row.scope))
            .min_by_key(|row| row.created_at);
        Ok(next.map(|row| {
            row.state = State::Running;
            row.started_at = Some(Utc::now());
            row.runner_job = Some(job_name(&row.id));
            row.clone()
        }))
    }

    async fn request_cancel(&self, id: &str) -> Result<Option<Operation>, GantryError> {
        let mut rows = self.rows();
        let Some(row) = rows.iter_mut().find(|row| row.id == id) else {
            return Ok(None);
        };
        match row.state {
            State::Queued => {
                row.state = State::Cancelled;
                row.finished_at = Some(Utc::now());
            }
            State::Running => row.cancel_requested = true,
            _ => {}
        }
        Ok(Some(row.clone()))
    }

    async fn finish(
        &self,
        id: &str,
        state: State,
        error: Option<&str>,
        log: Option<&str>,
        result: Option<serde_json::Value>,
    ) -> Result<(), GantryError> {
        if let Some(row) = self.rows().iter_mut().find(|row| row.id == id) {
            row.state = state;
            row.error = error.map(str::to_string);
            row.log = log.map(str::to_string);
            row.result = result;
            row.finished_at = Some(Utc::now());
        }
        Ok(())
    }

    async fn all_inventories(&self) -> Result<BTreeMap<String, Inventory>, GantryError> {
        Ok(self
            .inventories
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone())
    }

    async fn set_inventory(&self, package: &str, inventory: &Inventory) -> Result<(), GantryError> {
        self.inventories
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(package.to_string(), inventory.clone());
        Ok(())
    }

    async fn deployment_states(&self) -> Result<BTreeMap<String, String>, GantryError> {
        Ok(self
            .states
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone())
    }

    async fn set_deployment_state(
        &self,
        deployment: &str,
        desired: &str,
        _by: &str,
    ) -> Result<(), GantryError> {
        self.states
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(deployment.to_string(), desired.to_string());
        Ok(())
    }

    async fn save_plan(&self, new: &NewPlan) -> Result<StoredPlan, GantryError> {
        let plan = StoredPlan {
            id: Uuid::new_v4().to_string(),
            target: new.target.clone(),
            action: new.action.clone(),
            version: new.version.clone(),
            basis: new.basis.clone(),
            plan: new.plan.clone(),
            created_by: new.created_by.clone(),
            created_at: Utc::now(),
            operation_id: None,
        };
        self.plans
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(plan.clone());
        Ok(plan)
    }

    async fn plan(&self, id: &str) -> Result<Option<StoredPlan>, GantryError> {
        Ok(self
            .plans
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .find(|plan| plan.id == id)
            .cloned())
    }

    async fn claim_plan(&self, id: &str) -> Result<bool, GantryError> {
        let mut plans = self
            .plans
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match plans.iter_mut().find(|plan| plan.id == id) {
            Some(plan) if plan.operation_id.is_none() => {
                plan.operation_id = Some(CLAIMING.to_string());
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    async fn settle_plan(&self, id: &str, operation_id: Option<&str>) -> Result<(), GantryError> {
        if let Some(plan) = self
            .plans
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter_mut()
            .find(|plan| plan.id == id)
        {
            plan.operation_id = operation_id.map(str::to_string);
        }
        Ok(())
    }
}

// ---------------------------------------------------------------- postgres

const COLUMNS: &str = "id, kind, title, scope, state, plan, result, runner_job, requested_by, \
                       cancel_requested, error, log, created_at, started_at, finished_at";

pub struct PgStore {
    db: Db,
}

impl PgStore {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
}

fn from_row(row: &sqlx::postgres::PgRow) -> Result<Operation, GantryError> {
    let state: String = row.try_get("state")?;
    let plan: serde_json::Value = row.try_get("plan")?;
    Ok(Operation {
        id: row.try_get("id")?,
        kind: row.try_get("kind")?,
        title: row.try_get("title")?,
        scope: row.try_get("scope")?,
        // The CHECK constraint keeps this to the six known values.
        state: State::parse(&state).unwrap_or(State::Failed),
        plan: serde_json::from_value(plan)?,
        result: row.try_get("result")?,
        runner_job: row.try_get("runner_job")?,
        requested_by: row.try_get("requested_by")?,
        cancel_requested: row.try_get("cancel_requested")?,
        error: row.try_get("error")?,
        log: row.try_get("log")?,
        created_at: row.try_get("created_at")?,
        started_at: row.try_get("started_at")?,
        finished_at: row.try_get("finished_at")?,
    })
}

#[async_trait]
impl Store for PgStore {
    async fn create(&self, new: &NewOperation) -> Result<Operation, GantryError> {
        let pool = pool(&self.db)?;
        let schema = schema();
        let sql = format!(
            "INSERT INTO {schema}.operations (id, kind, title, scope, plan, requested_by) \
             VALUES ($1, $2, $3, $4, $5, $6) RETURNING {COLUMNS}"
        );
        let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(Uuid::new_v4().to_string())
            .bind(&new.kind)
            .bind(&new.title)
            .bind(&new.scope)
            .bind(serde_json::to_value(&new.plan)?)
            .bind(&new.requested_by)
            .fetch_one(pool)
            .await?;
        from_row(&row)
    }

    async fn get(&self, id: &str) -> Result<Option<Operation>, GantryError> {
        let pool = pool(&self.db)?;
        let schema = schema();
        let sql = format!("SELECT {COLUMNS} FROM {schema}.operations WHERE id = $1");
        let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(id)
            .fetch_optional(pool)
            .await?;
        row.as_ref().map(from_row).transpose()
    }

    async fn list(&self, limit: i64) -> Result<Vec<Operation>, GantryError> {
        let pool = pool(&self.db)?;
        let schema = schema();
        let sql =
            format!("SELECT {COLUMNS} FROM {schema}.operations ORDER BY created_at DESC LIMIT $1");
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(limit)
            .fetch_all(pool)
            .await?;
        rows.iter().map(from_row).collect()
    }

    async fn running(&self) -> Result<Vec<Operation>, GantryError> {
        let pool = pool(&self.db)?;
        let schema = schema();
        let sql = format!(
            "SELECT {COLUMNS} FROM {schema}.operations WHERE state = 'running' ORDER BY created_at"
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .fetch_all(pool)
            .await?;
        rows.iter().map(from_row).collect()
    }

    async fn claim_next(&self) -> Result<Option<Operation>, GantryError> {
        let pool = pool(&self.db)?;
        let schema = schema();

        // The oldest queued operation in a scope with nothing running. If two services race, both pick the
        // same row; the unique index lets exactly one UPDATE through and the other gets a violation.
        let pick = format!(
            "SELECT id FROM {schema}.operations q WHERE q.state = 'queued' AND NOT EXISTS \
             (SELECT 1 FROM {schema}.operations r WHERE r.scope = q.scope AND r.state = 'running') \
             ORDER BY q.created_at LIMIT 1"
        );
        let Some(id): Option<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(pick.as_str()))
            .fetch_optional(pool)
            .await?
        else {
            return Ok(None);
        };

        let claim = format!(
            "UPDATE {schema}.operations SET state = 'running', started_at = NOW(), runner_job = $2 \
             WHERE id = $1 AND state = 'queued' RETURNING {COLUMNS}"
        );
        let claimed = sqlx::query(sqlx::AssertSqlSafe(claim.as_str()))
            .bind(&id)
            .bind(job_name(&id))
            .fetch_optional(pool)
            .await;
        match claimed {
            Ok(row) => row.as_ref().map(from_row).transpose(),
            Err(error) => {
                let error = GantryError::from(error);
                if error.is_unique_violation() {
                    Ok(None)
                } else {
                    Err(error)
                }
            }
        }
    }

    async fn request_cancel(&self, id: &str) -> Result<Option<Operation>, GantryError> {
        let pool = pool(&self.db)?;
        let schema = schema();
        let sql = format!(
            "UPDATE {schema}.operations SET \
               state = CASE WHEN state = 'queued' THEN 'cancelled' ELSE state END, \
               finished_at = CASE WHEN state = 'queued' THEN NOW() ELSE finished_at END, \
               cancel_requested = cancel_requested OR state = 'running' \
             WHERE id = $1 RETURNING {COLUMNS}"
        );
        let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(id)
            .fetch_optional(pool)
            .await?;
        row.as_ref().map(from_row).transpose()
    }

    async fn finish(
        &self,
        id: &str,
        state: State,
        error: Option<&str>,
        log: Option<&str>,
        result: Option<serde_json::Value>,
    ) -> Result<(), GantryError> {
        let pool = pool(&self.db)?;
        let schema = schema();
        let sql = format!(
            "UPDATE {schema}.operations SET state = $2, error = $3, log = $4, result = $5, \
             finished_at = NOW() WHERE id = $1"
        );
        sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(id)
            .bind(state.as_str())
            .bind(error)
            .bind(log)
            .bind(result)
            .execute(pool)
            .await?;
        Ok(())
    }

    async fn all_inventories(&self) -> Result<BTreeMap<String, Inventory>, GantryError> {
        let pool = pool(&self.db)?;
        let schema = schema();
        let sql = format!("SELECT package, version, resources FROM {schema}.inventory");
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .fetch_all(pool)
            .await?;
        let mut all = BTreeMap::new();
        for row in &rows {
            let resources: serde_json::Value = row.try_get("resources")?;
            all.insert(
                row.try_get::<String, _>("package")?,
                Inventory {
                    version: row.try_get("version")?,
                    resources: serde_json::from_value(resources)?,
                },
            );
        }
        Ok(all)
    }

    async fn set_inventory(&self, package: &str, inventory: &Inventory) -> Result<(), GantryError> {
        let pool = pool(&self.db)?;
        let schema = schema();
        let sql = format!(
            "INSERT INTO {schema}.inventory (package, version, resources) VALUES ($1, $2, $3) \
             ON CONFLICT (package) DO UPDATE SET version = $2, resources = $3, updated_at = NOW()"
        );
        sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(package)
            .bind(&inventory.version)
            .bind(serde_json::to_value(&inventory.resources)?)
            .execute(pool)
            .await?;
        Ok(())
    }

    async fn deployment_states(&self) -> Result<BTreeMap<String, String>, GantryError> {
        let pool = pool(&self.db)?;
        let schema = schema();
        let sql = format!("SELECT deployment, desired FROM {schema}.deployment_states");
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .fetch_all(pool)
            .await?;
        rows.iter()
            .map(|row| Ok((row.try_get("deployment")?, row.try_get("desired")?)))
            .collect()
    }

    async fn set_deployment_state(
        &self,
        deployment: &str,
        desired: &str,
        by: &str,
    ) -> Result<(), GantryError> {
        let pool = pool(&self.db)?;
        let schema = schema();
        let sql = format!(
            "INSERT INTO {schema}.deployment_states (deployment, desired, updated_by) \
             VALUES ($1, $2, $3) \
             ON CONFLICT (deployment) DO UPDATE SET desired = $2, updated_by = $3, updated_at = NOW()"
        );
        sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(deployment)
            .bind(desired)
            .bind(by)
            .execute(pool)
            .await?;
        Ok(())
    }

    async fn save_plan(&self, new: &NewPlan) -> Result<StoredPlan, GantryError> {
        let pool = pool(&self.db)?;
        let schema = schema();
        let sql = format!(
            "INSERT INTO {schema}.plans (id, target, action, version, basis, plan, created_by) \
             VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING {PLAN_COLUMNS}"
        );
        let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(Uuid::new_v4().to_string())
            .bind(&new.target)
            .bind(&new.action)
            .bind(&new.version)
            .bind(&new.basis)
            .bind(serde_json::to_value(&new.plan)?)
            .bind(&new.created_by)
            .fetch_one(pool)
            .await?;
        plan_from_row(&row)
    }

    async fn plan(&self, id: &str) -> Result<Option<StoredPlan>, GantryError> {
        let pool = pool(&self.db)?;
        let schema = schema();
        let sql = format!("SELECT {PLAN_COLUMNS} FROM {schema}.plans WHERE id = $1");
        let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(id)
            .fetch_optional(pool)
            .await?;
        row.as_ref().map(plan_from_row).transpose()
    }

    async fn claim_plan(&self, id: &str) -> Result<bool, GantryError> {
        let pool = pool(&self.db)?;
        let schema = schema();
        let sql = format!(
            "UPDATE {schema}.plans SET operation_id = $2 WHERE id = $1 AND operation_id IS NULL"
        );
        let done = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(id)
            .bind(CLAIMING)
            .execute(pool)
            .await?;
        Ok(done.rows_affected() == 1)
    }

    async fn settle_plan(&self, id: &str, operation_id: Option<&str>) -> Result<(), GantryError> {
        let pool = pool(&self.db)?;
        let schema = schema();
        let sql = format!("UPDATE {schema}.plans SET operation_id = $2 WHERE id = $1");
        sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(id)
            .bind(operation_id)
            .execute(pool)
            .await?;
        Ok(())
    }
}

const PLAN_COLUMNS: &str =
    "id, target, action, version, basis, plan, created_by, created_at, operation_id";

fn plan_from_row(row: &sqlx::postgres::PgRow) -> Result<StoredPlan, GantryError> {
    let plan: serde_json::Value = row.try_get("plan")?;
    Ok(StoredPlan {
        id: row.try_get("id")?,
        target: row.try_get("target")?,
        action: row.try_get("action")?,
        version: row.try_get("version")?,
        basis: row.try_get("basis")?,
        plan: serde_json::from_value(plan)?,
        created_by: row.try_get("created_by")?,
        created_at: row.try_get("created_at")?,
        operation_id: row.try_get("operation_id")?,
    })
}
