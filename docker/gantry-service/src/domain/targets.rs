//! Targets: the packages Warehouse publishes, set beside what the cluster is actually running of each.
//! Never stored - always assembled from the two, so it cannot drift from either.

use crate::domain::cluster::Workload;
use crate::domain::registry::{PackageVersion, compare};
use serde::Serialize;
use std::cmp::Ordering;
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Published, nothing of it running.
    NotInstalled,
    /// Running the newest published version.
    Current,
    /// Running an older version than the newest published one.
    UpdateAvailable,
    /// Running something newer than the newest non-yanked version (that version was yanked, or this is
    /// a build from elsewhere).
    Ahead,
    /// Its units are on different versions: an install that did not finish.
    Mixed,
    /// Running, but no longer published (or never was): installed by hand.
    Unlisted,
}

#[derive(Clone, Debug, Serialize)]
pub struct Unit {
    pub kind: String,
    pub name: String,
    pub namespace: String,
    pub version: Option<String>,
    pub desired: i32,
    pub ready: i32,
}

#[derive(Clone, Debug, Serialize)]
pub struct Target {
    pub name: String,
    pub description: Option<String>,
    /// The namespace the newest version deploys into by default.
    pub namespace: Option<String>,
    /// The version all its units run, when they agree.
    pub installed: Option<String>,
    /// The newest non-yanked version published.
    pub offered: Option<String>,
    pub status: Status,
    pub units: Vec<Unit>,
}

pub fn assemble(catalog: &[PackageVersion], workloads: &[Workload]) -> Vec<Target> {
    let mut names: BTreeSet<&str> = catalog.iter().map(|p| p.name.as_str()).collect();
    names.extend(workloads.iter().map(|w| w.package.as_str()));

    names
        .into_iter()
        .map(|name| {
            let published = catalog.iter().find(|p| p.name == name);
            let units: Vec<Unit> = workloads
                .iter()
                .filter(|w| w.package == name)
                .map(|w| Unit {
                    kind: w.kind.clone(),
                    name: w.name.clone(),
                    namespace: w.namespace.clone(),
                    version: w.version.clone(),
                    desired: w.desired,
                    ready: w.ready,
                })
                .collect();

            let versions: BTreeSet<&str> =
                units.iter().filter_map(|u| u.version.as_deref()).collect();
            let installed = (versions.len() == 1)
                .then(|| versions.iter().next().map(|v| (*v).to_string()))
                .flatten();
            let offered = published.map(|p| p.version.clone());

            let status = match (&installed, &offered) {
                _ if units.is_empty() => Status::NotInstalled,
                _ if versions.len() > 1 => Status::Mixed,
                (_, None) => Status::Unlisted,
                (None, _) => Status::Mixed,
                (Some(have), Some(offer)) => match compare(have, offer) {
                    Ordering::Equal => Status::Current,
                    Ordering::Less => Status::UpdateAvailable,
                    Ordering::Greater => Status::Ahead,
                },
            };

            Target {
                name: name.to_string(),
                description: published.and_then(|p| p.description.clone()),
                namespace: published.and_then(|p| p.namespace.clone()),
                installed,
                offered,
                status,
                units,
            }
        })
        .collect()
}
