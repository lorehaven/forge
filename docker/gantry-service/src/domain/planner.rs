//! From "put package P at version V" to an ordered list of steps, with the reasons written down.
//!
//! The planner reads the published versions, what the cluster runs and what was overridden by hand - and
//! nothing else. In particular it never reads a Secret: whether a package's variables are all supplied is
//! the runner's `Check` step, which has the Secret mounted and runs before anything changes.

use crate::domain::GantryError;
use crate::domain::cluster::ClusterError;
use crate::domain::operation::{NewPlan, StoredPlan};
use crate::domain::registry::{PackageVersion, RegistryError, compare, newest};
use crate::domain::service::Gantry;
use crate::domain::steps::{Plan, Step};
use crate::domain::targets::{self, Target};
use std::cmp::Ordering;
use std::collections::BTreeMap;

/// How long a runner may spend waiting on a rollout before the install is called failed.
const INSTALL_TIMEOUT_SECS: u64 = 600;

#[derive(Debug, thiserror::Error)]
pub enum PlanError {
    #[error("no package named '{0}' is published")]
    UnknownTarget(String),
    #[error("{0}")]
    Refused(String),
    #[error("{0}")]
    Stale(String),
    #[error(transparent)]
    Registry(#[from] RegistryError),
    #[error(transparent)]
    Cluster(#[from] ClusterError),
    #[error(transparent)]
    Store(#[from] GantryError),
}

/// What kind of change this is, which is also what permission it needs: going to an older version is a
/// `rollback`, anything else a `deploy`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Install,
    Upgrade,
    Reinstall,
    Downgrade,
    Stop,
    Start,
    /// A start that stops something else first.
    Swap,
}

impl Action {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Install => "install",
            Self::Upgrade => "upgrade",
            Self::Reinstall => "reinstall",
            Self::Downgrade => "downgrade",
            Self::Stop => "stop",
            Self::Start => "start",
            Self::Swap => "swap",
        }
    }

    pub fn permission(self) -> &'static str {
        match self {
            Self::Downgrade => "rollback",
            Self::Stop | Self::Start => "scale",
            Self::Swap => "activate",
            Self::Install | Self::Upgrade | Self::Reinstall => "deploy",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "install" => Self::Install,
            "upgrade" => Self::Upgrade,
            "reinstall" => Self::Reinstall,
            "downgrade" => Self::Downgrade,
            "stop" => Self::Stop,
            "start" => Self::Start,
            "swap" => Self::Swap,
            _ => return None,
        })
    }
}

/// Everything about one target, as it is right now.
pub async fn describe(gantry: &Gantry, name: &str) -> Result<Option<Target>, PlanError> {
    Ok(all(gantry).await?.into_iter().find(|t| t.name == name))
}

pub async fn all(gantry: &Gantry) -> Result<Vec<Target>, PlanError> {
    let catalog = gantry.registry.catalog().await?;
    let workloads = gantry
        .cluster
        .list_workloads(&gantry.settings.allowed_namespaces)
        .await?;
    Ok(targets::assemble(&catalog, &workloads))
}

/// Works out the change that puts `name` at `version` (the newest non-yanked if `None`) and stores it.
/// A change worked out but not yet recorded or run: the steps, what they are, and what they were
/// worked out against.
#[derive(Clone, Debug)]
pub struct Prepared {
    pub target: String,
    pub action: Action,
    pub version: Option<String>,
    pub basis: Option<String>,
    pub plan: Plan,
}

impl Prepared {
    /// Run it now, with no plan stored and nothing to confirm: the operation and its log are the record.
    pub async fn run(
        self,
        gantry: &Gantry,
        requested_by: &str,
    ) -> Result<crate::domain::operation::Operation, PlanError> {
        let title = if self.plan.title.is_empty() {
            format!("{} {}", self.action.as_str(), self.target)
        } else {
            self.plan.title.clone()
        };
        gantry
            .submit(self.action.as_str(), &title, self.plan, requested_by)
            .await
            .map_err(|error| match error {
                crate::domain::service::SubmitError::Invalid(reason) => PlanError::Refused(reason),
                crate::domain::service::SubmitError::Store(e) => PlanError::Store(e),
            })
    }

    /// Record it as a plan to look at and confirm later (the API's way of previewing).
    pub async fn store(
        self,
        gantry: &Gantry,
        requested_by: &str,
    ) -> Result<(StoredPlan, Action), PlanError> {
        let action = self.action;
        let stored = gantry
            .store
            .save_plan(&NewPlan {
                target: self.target,
                action: action.as_str().to_string(),
                version: self.version,
                basis: self.basis,
                plan: self.plan,
                created_by: requested_by.to_string(),
            })
            .await?;
        Ok((stored, action))
    }
}

/// Works out the change that puts `name` at `version` (the newest non-yanked if `None`) and stores it.
pub async fn plan(
    gantry: &Gantry,
    name: &str,
    version: Option<&str>,
    requested_by: &str,
) -> Result<(StoredPlan, Action), PlanError> {
    prepare(gantry, name, version)
        .await?
        .store(gantry, requested_by)
        .await
}

pub async fn prepare(
    gantry: &Gantry,
    name: &str,
    version: Option<&str>,
) -> Result<Prepared, PlanError> {
    let versions = gantry.registry.versions(name).await?;
    if versions.is_empty() {
        return Err(PlanError::UnknownTarget(name.to_string()));
    }

    let target = describe(gantry, name).await?;
    let installed = target.as_ref().and_then(|t| t.installed.clone());
    let units = target.as_ref().map(|t| t.units.clone()).unwrap_or_default();

    let chosen = choose(&versions, version, installed.as_deref())?;
    let action = match installed.as_deref() {
        None => Action::Install,
        Some(have) => match compare(&chosen.version, have) {
            Ordering::Greater => Action::Upgrade,
            Ordering::Equal => Action::Reinstall,
            Ordering::Less => Action::Downgrade,
        },
    };

    let namespace = chosen
        .namespace
        .clone()
        .or_else(|| units.first().map(|u| u.namespace.clone()))
        .ok_or_else(|| {
            PlanError::Refused(format!(
                "{name} {} declares no namespace, and nothing of it is running to learn one from",
                chosen.version
            ))
        })?;

    let states = gantry.store.deployment_states().await?;
    let held = crate::domain::deployments::held_stopped(chosen, &states);
    let plan = build(
        gantry,
        &Subject {
            name,
            version: &chosen.version,
            namespace: &namespace,
            installed: installed.as_deref(),
            action,
        },
        &units,
        &held,
    );

    plan.validate(&gantry.settings.allowed_namespaces)
        .map_err(PlanError::Refused)?;

    Ok(Prepared {
        target: name.to_string(),
        action,
        version: Some(chosen.version.clone()),
        basis: installed,
        plan,
    })
}

fn choose<'a>(
    versions: &'a [PackageVersion],
    requested: Option<&str>,
    installed: Option<&str>,
) -> Result<&'a PackageVersion, PlanError> {
    match requested {
        None => newest(versions).ok_or_else(|| {
            PlanError::Refused(
                "every published version has been yanked; name one explicitly".to_string(),
            )
        }),
        Some(wanted) => {
            let found = versions
                .iter()
                .find(|v| v.version == wanted)
                .ok_or_else(|| PlanError::Refused(format!("version {wanted} is not published")))?;
            // A yanked version is withdrawn, not deleted: it can stay where it already runs, never go
            // anywhere new.
            if found.yanked && installed != Some(wanted) {
                return Err(PlanError::Refused(format!(
                    "version {wanted} has been yanked and cannot be installed"
                )));
            }
            Ok(found)
        }
    }
}

/// What a plan is for.
struct Subject<'a> {
    name: &'a str,
    version: &'a str,
    namespace: &'a str,
    installed: Option<&'a str>,
    action: Action,
}

fn build(
    gantry: &Gantry,
    subject: &Subject<'_>,
    units: &[crate::domain::targets::Unit],
    held: &[(String, Vec<String>)],
) -> Plan {
    let Subject {
        name,
        version,
        namespace,
        installed,
        action,
    } = *subject;
    // A deployment that is stopped stays stopped through an install or an update: its workloads are left
    // out of it, so the update changes what it *will* run (the inventory is the new version's) and starts
    // nothing.
    let held_back: Vec<String> = held
        .iter()
        .flat_map(|(_, resources)| resources.iter().cloned())
        .collect();

    // Gantry's own Deployment may be among the units. Replacing it ends this very process's pod, so it is
    // applied last and on its own, without waiting, and a guarded rollout follows it: the runner Job
    // outlives the pod it replaces, and it - not the broken service - owns the way back.
    let own = gantry.settings.self_deployment.as_ref();
    let own_resource = own.filter(|(ns, own_name)| {
        units
            .iter()
            .any(|u| u.kind == "deployment" && &u.namespace == ns && &u.name == own_name)
    });

    let mut steps = vec![
        Step::Pull {
            package: name.to_string(),
            version: version.to_string(),
        },
        Step::Check {
            package: name.to_string(),
            version: version.to_string(),
            values_secret: None,
        },
        Step::Install {
            package: name.to_string(),
            version: version.to_string(),
            namespace: namespace.to_string(),
            sets: BTreeMap::new(),
            replicas: BTreeMap::new(),
            values_secret: None,
            timeout_secs: Some(INSTALL_TIMEOUT_SECS),
            targets: Vec::new(),
            except: own_resource
                .map(|(_, own_name)| format!("deployment/{own_name}"))
                .into_iter()
                .chain(held_back.iter().cloned())
                .collect(),
            no_wait: false,
        },
    ];

    let mut summary = vec![match (action, installed) {
        (Action::Install, _) => format!("Install {name} {version} into {namespace}."),
        (Action::Upgrade, Some(have)) => format!("Upgrade {name} from {have} to {version}."),
        (Action::Downgrade, Some(have)) => format!("Roll {name} back from {have} to {version}."),
        (_, _) => format!("Re-apply {name} {version}."),
    }];
    for (deployment, _) in held {
        summary.push(format!("{deployment} is stopped and stays stopped."));
    }

    if let Some((ns, own_name)) = own_resource {
        let own_key = format!("deployment/{own_name}");
        steps.push(Step::Install {
            package: name.to_string(),
            version: version.to_string(),
            namespace: namespace.to_string(),
            sets: BTreeMap::new(),
            replicas: BTreeMap::new(),
            values_secret: None,
            timeout_secs: Some(INSTALL_TIMEOUT_SECS),
            targets: vec![own_key],
            except: Vec::new(),
            no_wait: true,
        });
        steps.push(Step::Rollout {
            namespace: ns.clone(),
            kind: "deployment".to_string(),
            name: own_name.clone(),
            timeout_secs: gantry.settings.self_rollout_timeout_secs,
            rollback_on_failure: true,
        });
        summary.push(format!(
            "Includes Gantry itself ({ns}/{own_name}), applied last. It is rolled back if it does not come up."
        ));
    }

    let title = match action {
        Action::Install => format!("Install {name} {version}"),
        Action::Upgrade => format!("Upgrade {name} → {version}"),
        Action::Downgrade => format!("Roll back {name} → {version}"),
        _ => format!("Re-apply {name} {version}"),
    };
    Plan {
        steps,
        title,
        summary,
        touches: vec![name.to_string()],
        ..Plan::default()
    }
}
/// Turns a stored plan into a queued operation, once, if it still describes the cluster.
pub async fn confirm(
    gantry: &Gantry,
    plan_id: &str,
    requested_by: &str,
) -> Result<crate::domain::operation::Operation, PlanError> {
    let stored = gantry
        .store
        .plan(plan_id)
        .await?
        .ok_or_else(|| PlanError::UnknownTarget(plan_id.to_string()))?;
    if stored.operation_id.is_some() {
        return Err(PlanError::Stale(
            "this plan has already been confirmed".to_string(),
        ));
    }

    // The plan is only true of the cluster it was made against.
    let now = if stored.plan.deployments.is_empty() {
        describe(gantry, &stored.target)
            .await?
            .and_then(|target| target.installed)
    } else {
        let views = crate::domain::deployments::all(gantry).await?;
        Some(crate::domain::deployments::signature(
            &views,
            &stored.plan.deployments,
        ))
    };
    if now != stored.basis {
        return Err(PlanError::Stale(format!(
            "{} is now at {}, not {} as when this was planned; plan it again",
            stored.target,
            now.as_deref().unwrap_or("nothing"),
            stored.basis.as_deref().unwrap_or("nothing")
        )));
    }

    if !gantry.store.claim_plan(plan_id).await? {
        return Err(PlanError::Stale(
            "this plan has already been confirmed".to_string(),
        ));
    }
    let title = if stored.plan.title.is_empty() {
        stored
            .plan
            .summary
            .first()
            .cloned()
            .unwrap_or_else(|| format!("{} {}", stored.action, stored.target))
    } else {
        stored.plan.title.clone()
    };
    match gantry
        .submit(&stored.action, &title, stored.plan.clone(), requested_by)
        .await
    {
        Ok(operation) => {
            gantry
                .store
                .settle_plan(plan_id, Some(&operation.id))
                .await?;
            Ok(operation)
        }
        Err(error) => {
            gantry.store.settle_plan(plan_id, None).await?;
            Err(match error {
                crate::domain::service::SubmitError::Invalid(reason) => PlanError::Refused(reason),
                crate::domain::service::SubmitError::Store(e) => PlanError::Store(e),
            })
        }
    }
}
