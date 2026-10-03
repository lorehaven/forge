//! Deployments: the things that can be stopped and started on their own - inference as one, training as
//! another. Which exist comes from the packages (`[[deployment]]` in `rivet.toml`); whether each is up comes
//! from the cluster; what each is *meant* to be comes from what was last done to it (or its default).
//!
//! Stopping is scaling to zero, never deleting: what a deployment is stays installed and versioned, so
//! starting it again is quick and an upgrade of it while it is down changes what it will run, starts nothing.

use crate::domain::GantryError;
use crate::domain::cluster::Workload;
use crate::domain::operation::{Inventory, StoredPlan};
use crate::domain::planner::{Action, PlanError};
use crate::domain::registry::PackageVersion;
use crate::domain::service::Gantry;
use crate::domain::steps::{Effect, Plan, Step};
use rivet_package::{AlsoStops, DefaultState};
use serde::Serialize;
use std::collections::BTreeMap;

const INSTALL_TIMEOUT_SECS: u64 = 600;
const POD_GONE_TIMEOUT_SECS: u64 = 300;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Observed {
    /// Every workload is up.
    Running,
    /// Every workload is scaled to zero.
    Stopped,
    /// Some up, some not: an operation that did not finish, or something changed by hand.
    Partial,
    /// None of its workloads exist: the package is not installed.
    Absent,
}

#[derive(Clone, Debug, Serialize)]
pub struct ResourceView {
    pub kind: String,
    pub name: String,
    pub namespace: Option<String>,
    pub desired: Option<i32>,
    pub ready: Option<i32>,
    pub present: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct DeploymentView {
    pub name: String,
    /// Declared in the package's manifest, as opposed to the implicit one-per-package fallback.
    pub declared: bool,
    /// The package it belongs to.
    pub target: String,
    pub description: Option<String>,
    pub resources: Vec<ResourceView>,
    pub default: String,
    /// What it is meant to be: what was last done to it, or its default.
    pub desired: String,
    pub observed: Observed,
    /// Meant to be one thing, is another.
    pub drift: bool,
    pub conflicts_with: Vec<String>,
    /// The ones in `conflicts_with` (either direction) that are up right now.
    pub conflicting_running: Vec<String>,
    #[serde(skip)]
    pub also_stops: Vec<AlsoStops>,
    #[serde(skip)]
    pub version: Option<String>,
}

fn state_name(state: DefaultState) -> &'static str {
    match state {
        DefaultState::Running => "running",
        DefaultState::Stopped => "stopped",
    }
}

/// Every deployment, from the packages' declarations, the cluster and the recorded states.
pub fn assemble(
    catalog: &[PackageVersion],
    workloads: &[Workload],
    states: &BTreeMap<String, String>,
    inventories: &BTreeMap<String, Inventory>,
) -> Vec<DeploymentView> {
    let mut views: Vec<DeploymentView> = Vec::new();
    let mut packages: Vec<&str> = catalog.iter().map(|p| p.name.as_str()).collect();
    for package in inventories.keys() {
        if !packages.contains(&package.as_str()) {
            packages.push(package.as_str());
        }
    }
    for workload in workloads {
        if !packages.contains(&workload.package.as_str()) {
            packages.push(workload.package.as_str());
        }
    }
    packages.sort_unstable();

    for package in packages {
        let published = catalog.iter().find(|p| p.name == package);
        let theirs: Vec<&Workload> = workloads.iter().filter(|w| w.package == package).collect();
        let inventory = inventories.get(package);
        // The version running; failing that (everything is stopped, so nothing is left to read it from),
        // the one the package was last installed at.
        let installed = installed_version(&theirs).or_else(|| inventory.map(|i| i.version.clone()));
        let default_namespace = published.and_then(|p| p.namespace.clone());

        let mut declared = published
            .map(PackageVersion::deployments)
            .unwrap_or_default();
        let explicit = !declared.is_empty();
        if declared.is_empty() {
            // No declaration: the whole package is one deployment, named after it - its workloads, as the
            // inventory knows them (so stopping all of them does not make the deployment vanish).
            let mut names: Vec<String> = inventory
                .into_iter()
                .flat_map(|i| i.resources.iter())
                .filter(|item| crate::domain::resources::is_workload(&item.kind))
                .map(|item| format!("{}/{}", item.kind.to_ascii_lowercase(), item.name))
                .collect();
            for w in &theirs {
                let key = format!("{}/{}", w.kind, w.name);
                if !names.contains(&key) {
                    names.push(key);
                }
            }
            names.retain(|r| r.starts_with("deployment/") || r.starts_with("statefulset/"));
            if names.is_empty() {
                continue;
            }
            declared.push(rivet_package::Deployment {
                name: package.to_string(),
                description: published.and_then(|p| p.description.clone()),
                resources: names,
                default: DefaultState::Running,
                conflicts_with: Vec::new(),
                also_stops: Vec::new(),
            });
        }

        for deployment in declared {
            if views.iter().any(|v| v.name == deployment.name) {
                tracing::warn!(
                    "deployment '{}' is declared by more than one package; keeping the first",
                    deployment.name
                );
                continue;
            }
            let resources: Vec<ResourceView> = deployment
                .resources
                .iter()
                .map(|resource| {
                    let (kind, name) = resource.split_once('/').unwrap_or(("", resource));
                    let found = theirs.iter().find(|w| w.kind == kind && w.name == name);
                    let from_inventory = inventory.and_then(|i| {
                        i.resources
                            .iter()
                            .find(|item| item.kind.eq_ignore_ascii_case(kind) && item.name == name)
                    });
                    ResourceView {
                        kind: kind.to_string(),
                        name: name.to_string(),
                        // Where it is, or where it would be put back.
                        namespace: found
                            .map(|w| w.namespace.clone())
                            .or_else(|| from_inventory.and_then(|i| i.namespace.clone()))
                            .or_else(|| default_namespace.clone()),
                        desired: found.map(|w| w.desired),
                        ready: found.map(|w| w.ready),
                        present: found.is_some(),
                    }
                })
                .collect();

            let default = state_name(deployment.default).to_string();
            let desired = states
                .get(&deployment.name)
                .cloned()
                .unwrap_or_else(|| default.clone());
            let observed = observe(&resources, !theirs.is_empty() || inventory.is_some());
            let drift = match observed {
                Observed::Running => desired != "running",
                Observed::Stopped => desired != "stopped",
                Observed::Partial => true,
                Observed::Absent => false,
            };
            views.push(DeploymentView {
                declared: explicit,
                name: deployment.name,
                target: package.to_string(),
                description: deployment.description,
                resources,
                default,
                desired,
                observed,
                drift,
                conflicts_with: deployment.conflicts_with,
                conflicting_running: Vec::new(),
                also_stops: deployment.also_stops,
                version: installed.clone(),
            });
        }
    }

    // A conflict is mutual whichever side declared it.
    let snapshot: Vec<(String, Observed, Vec<String>)> = views
        .iter()
        .map(|v| (v.name.clone(), v.observed, v.conflicts_with.clone()))
        .collect();
    for view in &mut views {
        let mut running: Vec<String> = snapshot
            .iter()
            .filter(|(other, observed, theirs)| {
                *other != view.name
                    && matches!(observed, Observed::Running | Observed::Partial)
                    && (view.conflicts_with.contains(other) || theirs.contains(&view.name))
            })
            .map(|(other, _, _)| other.clone())
            .collect();
        running.sort();
        view.conflicting_running = running;
    }
    views
}

fn installed_version(workloads: &[&Workload]) -> Option<String> {
    let mut versions: Vec<&str> = workloads
        .iter()
        .filter_map(|w| w.version.as_deref())
        .collect();
    versions.sort_unstable();
    versions.dedup();
    (versions.len() == 1).then(|| versions[0].to_string())
}

/// Up, stopped (deleted, or scaled to nothing), partly one or the other, or not there at all.
///
/// `installed` is whether the package is installed at all: stopping a deployment deletes its workloads,
/// so "none of them exist" is *stopped* for a package that is installed and *absent* for one that is not.
fn observe(resources: &[ResourceView], installed: bool) -> Observed {
    let present: Vec<&ResourceView> = resources.iter().filter(|r| r.present).collect();
    if present.is_empty() {
        return if installed {
            Observed::Stopped
        } else {
            Observed::Absent
        };
    }
    let up = present
        .iter()
        .filter(|r| r.desired.unwrap_or(0) > 0)
        .count();
    if present.len() < resources.len() {
        Observed::Partial
    } else if up == present.len() {
        Observed::Running
    } else if up == 0 {
        Observed::Stopped
    } else {
        Observed::Partial
    }
}

pub async fn all(gantry: &Gantry) -> Result<Vec<DeploymentView>, PlanError> {
    let catalog = gantry.registry.catalog().await?;
    let workloads = gantry
        .cluster
        .list_workloads(&gantry.settings.allowed_namespaces)
        .await?;
    let states = gantry.store.deployment_states().await?;
    let inventories = gantry.store.all_inventories().await?;
    Ok(assemble(&catalog, &workloads, &states, &inventories))
}

/// The resources (`kind/name`) of `package`'s deployments that are meant to be stopped. An install or an
/// upgrade applies these at zero, which is how a stopped deployment survives its package being updated.
pub fn held_stopped(
    package: &PackageVersion,
    states: &BTreeMap<String, String>,
) -> Vec<(String, Vec<String>)> {
    package
        .deployments()
        .into_iter()
        .filter(|d| {
            states
                .get(&d.name)
                .map_or(d.default == DefaultState::Stopped, |s| s == "stopped")
        })
        .map(|d| (d.name, d.resources))
        .collect()
}

/// What a plan for these deployments is made against: each one's observed state, so a confirm can tell
/// the cluster has moved.
pub fn signature(views: &[DeploymentView], names: &[String]) -> String {
    let mut parts: Vec<String> = names
        .iter()
        .map(|name| {
            let observed =
                views
                    .iter()
                    .find(|v| &v.name == name)
                    .map_or("unknown".to_string(), |v| {
                        serde_json::to_value(v.observed)
                            .ok()
                            .and_then(|o| o.as_str().map(str::to_string))
                            .unwrap_or_default()
                    });
            format!("{name}={observed}")
        })
        .collect();
    parts.sort();
    parts.join(",")
}

/// What was asked of a deployment.
#[derive(Clone, Debug)]
pub enum Request {
    Stop,
    /// Start it, also stopping these first (on top of any it declares a conflict with).
    Start {
        also_stop: Vec<String>,
    },
}

pub async fn plan(
    gantry: &Gantry,
    name: &str,
    request: &Request,
    requested_by: &str,
) -> Result<(StoredPlan, Action), PlanError> {
    prepare(gantry, name, request)
        .await?
        .store(gantry, requested_by)
        .await
}

pub async fn prepare(
    gantry: &Gantry,
    name: &str,
    request: &Request,
) -> Result<crate::domain::planner::Prepared, PlanError> {
    let views = all(gantry).await?;
    let subject = find(&views, name)?;

    let mut steps = Vec::new();
    let mut summary = Vec::new();
    let mut effects = Vec::new();
    let mut involved = vec![subject.name.clone()];
    let mut touches = vec![subject.target.clone()];

    let action = match request {
        Request::Stop => {
            require(subject.observed != Observed::Absent, || {
                format!("{name} is not installed, so there is nothing to stop")
            })?;
            require(
                !(subject.observed == Observed::Stopped && subject.desired == "stopped"),
                || format!("{name} is already stopped"),
            )?;
            stop(gantry, subject, &mut steps, &mut summary, &mut effects)?;
            Action::Stop
        }
        Request::Start { also_stop } => {
            require(subject.observed != Observed::Absent, || {
                format!("{name} is not installed; install its package first")
            })?;

            let mut to_stop: Vec<&DeploymentView> = Vec::new();
            for other in subject.conflicting_running.iter().chain(also_stop.iter()) {
                let view = find(&views, other)?;
                require(view.name != subject.name, || {
                    format!("{name} cannot be stopped to start itself")
                })?;
                if !to_stop.iter().any(|v| v.name == view.name)
                    && view.observed != Observed::Stopped
                    && view.observed != Observed::Absent
                {
                    to_stop.push(view);
                }
            }
            require(
                !(subject.observed == Observed::Running
                    && subject.desired == "running"
                    && to_stop.is_empty()),
                || format!("{name} is already running"),
            )?;

            for view in &to_stop {
                stop(gantry, view, &mut steps, &mut summary, &mut effects)?;
                involved.push(view.name.clone());
                if !touches.contains(&view.target) {
                    touches.push(view.target.clone());
                }
            }
            start(subject, &mut steps, &mut summary, &mut effects)?;

            if to_stop.is_empty() {
                Action::Start
            } else {
                Action::Swap
            }
        }
    };

    let title = match action {
        Action::Stop => format!("Stop {}", subject.name),
        Action::Start => format!("Start {}", subject.name),
        _ => format!("Swap to {}", subject.name),
    };
    let plan = Plan {
        steps,
        title,
        summary,
        effects,
        touches: touches.clone(),
        deployments: involved.clone(),
    };
    plan.validate(&gantry.settings.allowed_namespaces)
        .map_err(PlanError::Refused)?;

    Ok(crate::domain::planner::Prepared {
        target: subject.target.clone(),
        action,
        version: subject.version.clone(),
        basis: Some(signature(&views, &involved)),
        plan,
    })
}

fn find<'a>(views: &'a [DeploymentView], name: &str) -> Result<&'a DeploymentView, PlanError> {
    views
        .iter()
        .find(|v| v.name == name)
        .ok_or_else(|| PlanError::UnknownTarget(name.to_string()))
}

fn require(condition: bool, message: impl FnOnce() -> String) -> Result<(), PlanError> {
    if condition {
        Ok(())
    } else {
        Err(PlanError::Refused(message()))
    }
}

/// Scale down in the order the deployment lists its resources (what keeps another alive goes first), then
/// remove the pods nothing owns, then wait until every pod is actually gone - a GPU is only free then.
fn stop(
    gantry: &Gantry,
    view: &DeploymentView,
    steps: &mut Vec<Step>,
    summary: &mut Vec<String>,
    effects: &mut Vec<Effect>,
) -> Result<(), PlanError> {
    for resource in &view.resources {
        if let Some((ns, _)) = gantry.settings.protected.iter().find(|(ns, n)| {
            resource.namespace.as_deref() == Some(ns.as_str()) && *n == resource.name
        }) {
            return Err(PlanError::Refused(format!(
                "{ns}/{} is protected and cannot be stopped: Gantry, its database and its login depend on it",
                resource.name
            )));
        }
    }

    let present: Vec<&ResourceView> = view.resources.iter().filter(|r| r.present).collect();
    // Stopping is deleting the workloads - what the package declares stays known, so starting is applying
    // them again from it. Scaling to zero would leave them there, half alive.
    for resource in &present {
        steps.push(Step::Delete {
            resource: crate::domain::resources::kubectl_name(
                Some("apps/v1"),
                &resource.kind,
                &resource.name,
            ),
            namespace: resource.namespace.clone(),
        });
    }
    for extra in &view.also_stops {
        steps.push(Step::DeletePods {
            selector: extra.selector.clone(),
            namespaces: extra.namespaces.clone(),
        });
    }
    // Their pods, found by the label riveter puts on every pod template; and the unowned ones by their own.
    for resource in &present {
        steps.push(Step::WaitGone {
            selector: format!("app.kubernetes.io/name={}", resource.name),
            namespaces: resource.namespace.clone().into_iter().collect(),
            timeout_secs: POD_GONE_TIMEOUT_SECS,
        });
    }
    for extra in &view.also_stops {
        steps.push(Step::WaitGone {
            selector: extra.selector.clone(),
            namespaces: extra.namespaces.clone(),
            timeout_secs: POD_GONE_TIMEOUT_SECS,
        });
    }

    let names: Vec<&str> = present.iter().map(|r| r.name.as_str()).collect();
    summary.push(format!("Stop {}: {}.", view.name, names.join(", ")));
    for extra in &view.also_stops {
        summary.push(format!("Delete the pods matching {}.", extra.selector));
    }
    effects.push(Effect::SetDesired {
        deployment: view.name.clone(),
        desired: "stopped".to_string(),
    });
    Ok(())
}

/// Re-render only this deployment's workloads from the installed package version, at the package's own
/// replica counts (plus anything set by hand), and wait for them.
fn start(
    view: &DeploymentView,
    steps: &mut Vec<Step>,
    summary: &mut Vec<String>,
    effects: &mut Vec<Effect>,
) -> Result<(), PlanError> {
    let version = view.version.clone().ok_or_else(|| {
        PlanError::Refused(format!(
            "{} is installed at more than one version (or none can be read); bring the package to one version first",
            view.target
        ))
    })?;
    let namespace = view
        .resources
        .iter()
        .find_map(|r| r.namespace.clone())
        .ok_or_else(|| PlanError::Refused(format!("{} has no workloads to start", view.name)))?;

    let targets: Vec<String> = view
        .resources
        .iter()
        .map(|r| format!("{}/{}", r.kind, r.name))
        .collect();
    steps.push(Step::Pull {
        package: view.target.clone(),
        version: version.clone(),
    });
    steps.push(Step::Check {
        package: view.target.clone(),
        version: version.clone(),
        values_secret: None,
    });
    steps.push(Step::Install {
        package: view.target.clone(),
        version: version.clone(),
        namespace,
        sets: std::collections::BTreeMap::new(),
        replicas: std::collections::BTreeMap::new(),
        values_secret: None,
        timeout_secs: Some(INSTALL_TIMEOUT_SECS),
        targets: targets.clone(),
        except: Vec::new(),
        no_wait: false,
    });
    let names: Vec<&str> = view.resources.iter().map(|r| r.name.as_str()).collect();
    summary.push(format!(
        "Start {}: {} (from {} {version}).",
        view.name,
        names.join(", "),
        view.target
    ));
    effects.push(Effect::SetDesired {
        deployment: view.name.clone(),
        desired: "running".to_string(),
    });
    Ok(())
}

/// The recorded state, applied by the reconciler once an operation has succeeded.
pub async fn apply_effects(gantry: &Gantry, plan: &Plan, by: &str) -> Result<(), GantryError> {
    for effect in &plan.effects {
        match effect {
            Effect::SetDesired {
                deployment,
                desired,
            } => {
                gantry
                    .store
                    .set_deployment_state(deployment, desired, by)
                    .await?
            }
        }
    }
    Ok(())
}
