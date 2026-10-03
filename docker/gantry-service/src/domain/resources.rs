//! Resources: every kind of thing a package put in the cluster, grouped by package, with what the package
//! says it should contain beside what is actually there.
//!
//! Two sources. The **cluster** says what exists (anything carrying the `riveter.forge/package` label, of
//! any kind). The **inventory** says what the package declares (recorded from the last successful install),
//! which is what makes a deleted resource visible - and re-appliable - instead of just gone.

use crate::domain::cluster::LiveResource;
use crate::domain::operation::Inventory;
use crate::domain::targets::{Status, Target};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// One resource a package declares, as an install reported it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventoryItem {
    #[serde(rename = "apiVersion", default)]
    pub api_version: Option<String>,
    pub kind: String,
    pub name: String,
    #[serde(default)]
    pub namespace: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    /// There, and as the package left it.
    Synced,
    /// There, but changed from Gantry since the package was installed. The next install overwrites it.
    Edited,
    /// The package declares it and the cluster does not have it (deleted - stopped).
    Missing,
    /// There, carrying the package's label, but the package no longer declares it.
    Extra,
    /// A Secret: its presence is not looked at and its value never is.
    Hidden,
}

impl State {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Synced => "synced",
            Self::Edited => "edited",
            Self::Missing => "missing",
            Self::Extra => "extra",
            Self::Hidden => "hidden",
        }
    }
}

impl SyncState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotInstalled => "not_installed",
            Self::Missing => "missing",
            Self::OutOfSync => "out_of_sync",
            Self::Synced => "synced",
            Self::Unlisted => "unlisted",
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Row {
    #[serde(rename = "apiVersion")]
    pub api_version: Option<String>,
    pub kind: String,
    pub name: String,
    pub namespace: Option<String>,
    pub state: State,
    /// `1/1` for a workload that reports it.
    pub ready: Option<String>,
    pub workload: bool,
    pub editable: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct Group {
    pub package: String,
    pub description: Option<String>,
    /// What the package's own record of itself says is installed.
    pub installed: Option<String>,
    pub offered: Option<String>,
    pub status: crate::domain::targets::Status,
    /// Whether an inventory exists, i.e. whether missing resources can be known about at all.
    pub inventoried: bool,
    pub rows: Vec<Row>,
}

/// An application's place against what its package says, as Argo CD puts it: is the cluster what the
/// package declares?
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncState {
    /// Published, nothing of it in the cluster.
    NotInstalled,
    /// Something the package declares is not in the cluster (deleted, or stopped).
    Missing,
    /// Everything is there, but something was edited or added by hand, or the package has a newer version.
    OutOfSync,
    /// What the package declares is what is running.
    Synced,
    /// Running, but no longer published.
    Unlisted,
}

/// What a card or a header says about one application, counted from its rows.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Summary {
    pub sync: SyncState,
    pub total: usize,
    pub synced: usize,
    pub edited: usize,
    pub missing: usize,
    pub extra: usize,
    pub workloads: usize,
    /// Workloads that are present and report fewer ready replicas than desired.
    pub not_ready: usize,
    /// The newer version the package offers, when the running one is behind.
    pub update_to: Option<String>,
}

impl Group {
    pub fn summary(&self) -> Summary {
        let count = |state: State| self.rows.iter().filter(|r| r.state == state).count();
        let missing = count(State::Missing);
        let edited = count(State::Edited);
        let extra = count(State::Extra);
        let present = self.rows.iter().filter(|r| r.state != State::Missing);
        let not_ready = present
            .clone()
            .filter(|r| r.workload)
            .filter(|r| {
                r.ready
                    .as_deref()
                    .and_then(|ready| ready.split_once('/'))
                    .and_then(|(have, want)| {
                        Some((have.parse::<i32>().ok()?, want.parse::<i32>().ok()?))
                    })
                    .is_some_and(|(have, want)| have < want)
            })
            .count();
        let update_to = (self.status == Status::UpdateAvailable)
            .then(|| self.offered.clone())
            .flatten();

        let sync = if present.count() == 0 && self.installed.is_none() {
            SyncState::NotInstalled
        } else if missing > 0 {
            SyncState::Missing
        } else if self.status == Status::Unlisted {
            SyncState::Unlisted
        } else if edited > 0
            || extra > 0
            || matches!(self.status, Status::UpdateAvailable | Status::Mixed)
        {
            SyncState::OutOfSync
        } else {
            SyncState::Synced
        };

        Summary {
            sync,
            total: self.rows.len(),
            synced: count(State::Synced) + count(State::Hidden),
            edited,
            missing,
            extra,
            workloads: self.rows.iter().filter(|r| r.workload).count(),
            not_ready,
            update_to,
        }
    }

    /// The rows that pass a filter: an empty `kind` or `state` matches everything, and `query` is a
    /// case-insensitive piece of the name.
    pub fn filtered(&self, kind: &str, state: &str, query: &str) -> Vec<&Row> {
        let query = query.trim().to_ascii_lowercase();
        self.rows
            .iter()
            .filter(|r| kind.is_empty() || r.kind.eq_ignore_ascii_case(kind))
            .filter(|r| state.is_empty() || r.state.as_str() == state)
            .filter(|r| query.is_empty() || r.name.to_ascii_lowercase().contains(&query))
            .collect()
    }

    /// The kinds present, for a filter.
    pub fn kinds(&self) -> Vec<String> {
        let mut kinds: Vec<String> = self.rows.iter().map(|r| r.kind.clone()).collect();
        kinds.sort_unstable();
        kinds.dedup();
        kinds
    }
}

pub fn is_workload(kind: &str) -> bool {
    matches!(
        kind.to_ascii_lowercase().as_str(),
        "deployment" | "statefulset" | "daemonset"
    )
}

pub fn is_secret(kind: &str) -> bool {
    kind.eq_ignore_ascii_case("secret")
}

fn key(kind: &str, name: &str, namespace: Option<&str>) -> (String, String, String) {
    (
        kind.to_ascii_lowercase(),
        name.to_string(),
        namespace.unwrap_or_default().to_string(),
    )
}

/// What `kubectl` calls a resource: `deployment.apps/name`, `clusterrole.rbac.authorization.k8s.io/name`.
/// The group disambiguates kinds that exist in more than one (a CRD called `Certificate` is not core's).
pub fn kubectl_name(api_version: Option<&str>, kind: &str, name: &str) -> String {
    let group = api_version
        .and_then(|v| v.split_once('/'))
        .map(|(group, _)| group)
        .filter(|group| !group.is_empty());
    match group {
        Some(group) => format!("{}.{group}/{name}", kind.to_ascii_lowercase()),
        None => format!("{}/{name}", kind.to_ascii_lowercase()),
    }
}

/// Groups by package: what the cluster has, what the inventory says should be there, and which is which.
pub fn group(
    targets: &[Target],
    live: &[LiveResource],
    inventories: &BTreeMap<String, Inventory>,
) -> Vec<Group> {
    let mut packages: Vec<&str> = targets.iter().map(|t| t.name.as_str()).collect();
    for resource in live {
        if !packages.contains(&resource.package.as_str()) {
            packages.push(resource.package.as_str());
        }
    }
    for package in inventories.keys() {
        if !packages.contains(&package.as_str()) {
            packages.push(package.as_str());
        }
    }
    packages.sort_unstable();
    packages.dedup();

    packages
        .into_iter()
        .map(|package| {
            let target = targets.iter().find(|t| t.name == package);
            let inventory = inventories.get(package);
            let theirs: Vec<&LiveResource> = live.iter().filter(|r| r.package == package).collect();

            let mut rows: Vec<Row> = Vec::new();
            let mut seen = std::collections::BTreeSet::new();

            for item in inventory.iter().flat_map(|i| i.resources.iter()) {
                let k = key(&item.kind, &item.name, item.namespace.as_deref());
                let found = theirs
                    .iter()
                    .find(|r| key(&r.kind, &r.name, r.namespace.as_deref()) == k);
                seen.insert(k);
                let state = match found {
                    _ if is_secret(&item.kind) => State::Hidden,
                    Some(r) if r.edited => State::Edited,
                    Some(_) => State::Synced,
                    None => State::Missing,
                };
                rows.push(Row {
                    api_version: found
                        .and_then(|r| r.api_version.clone())
                        .or_else(|| item.api_version.clone()),
                    kind: item.kind.clone(),
                    name: item.name.clone(),
                    namespace: item.namespace.clone(),
                    state,
                    ready: found.and_then(|r| r.ready.clone()),
                    workload: is_workload(&item.kind),
                    editable: found.is_some() && !is_secret(&item.kind),
                });
            }
            for resource in theirs {
                let k = key(
                    &resource.kind,
                    &resource.name,
                    resource.namespace.as_deref(),
                );
                if seen.contains(&k) {
                    continue;
                }
                rows.push(Row {
                    api_version: resource.api_version.clone(),
                    kind: resource.kind.clone(),
                    name: resource.name.clone(),
                    namespace: resource.namespace.clone(),
                    state: if resource.edited {
                        State::Edited
                    } else if inventory.is_some() {
                        State::Extra
                    } else {
                        State::Synced
                    },
                    ready: resource.ready.clone(),
                    workload: is_workload(&resource.kind),
                    editable: !is_secret(&resource.kind),
                });
            }

            // Workloads first (they are what is started and stopped), then by kind and name.
            rows.sort_by(|a, b| {
                (!a.workload, &a.kind, &a.name).cmp(&(!b.workload, &b.kind, &b.name))
            });

            Group {
                package: package.to_string(),
                description: target.and_then(|t| t.description.clone()),
                installed: target
                    .and_then(|t| t.installed.clone())
                    .or_else(|| inventory.map(|i| i.version.clone())),
                offered: target.and_then(|t| t.offered.clone()),
                status: target.map_or(crate::domain::targets::Status::Unlisted, |t| t.status),
                inventoried: inventory.is_some(),
                rows,
            }
        })
        .collect()
}

/// The package's own record of itself, from the last `riveter-inventory:` line in a runner's log.
pub fn inventory_from_log(log: &str) -> Option<Vec<InventoryItem>> {
    let line = log
        .lines()
        .rev()
        .find_map(|line| line.trim().strip_prefix("riveter-inventory:"))?;
    let items: Vec<InventoryItem> = serde_json::from_str(line.trim()).ok()?;
    (!items.is_empty()).then_some(items)
}
