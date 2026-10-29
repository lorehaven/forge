//! The install-state join: which programs are up to date, which can be updated, which are not installed.

use std::collections::BTreeMap;

use crate::model::{Artifact, Installed};

/// Where a program stands against the registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallState {
    NotInstalled,
    UpToDate,
    UpdateAvailable,
}

/// The state of a program given the version code on the machine (if any) and the registry's latest.
#[must_use]
pub const fn state_for(installed: Option<i64>, latest: i64) -> InstallState {
    match installed {
        None => InstallState::NotInstalled,
        Some(code) if code >= latest => InstallState::UpToDate,
        Some(_) => InstallState::UpdateAvailable,
    }
}

/// One row of the catalog: the registry's latest build, what is installed, and whether it is this app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub latest: Artifact,
    pub installed: Option<Installed>,
    /// The program is the running app itself (it updates itself the ordinary way, but is put last when
    /// updating everything: installing over itself ends the process).
    pub is_self: bool,
}

impl Entry {
    #[must_use]
    pub fn state(&self) -> InstallState {
        state_for(
            self.installed.as_ref().map(|i| i.version_code),
            self.latest.version_code,
        )
    }
}

/// The catalog joined with what is installed, in the order the screens show: updates first, then what is up to
/// date, then what is not installed; alphabetical by title within each group.
///
/// Only programs in the catalog are listed (this is a store, not a device manager); a catalogued program is
/// always listed, installed or not. A yanked build is never offered as the latest, whatever the registry sent.
#[must_use]
pub fn build_entries(
    catalog: &[Artifact],
    installed: &BTreeMap<String, Installed>,
    self_program: &str,
) -> Vec<Entry> {
    // One row per program: the highest non-yanked version code, if the registry sent several.
    let mut latest: BTreeMap<&str, &Artifact> = BTreeMap::new();
    for artifact in catalog.iter().filter(|a| !a.yanked) {
        let slot = latest.entry(artifact.program.as_str()).or_insert(artifact);
        if artifact.version_code > slot.version_code {
            *slot = artifact;
        }
    }
    let mut entries: Vec<Entry> = latest
        .into_values()
        .map(|artifact| {
            let mut latest = artifact.clone();
            let is_self = artifact.program == self_program;
            // Builds published before the registry was told this app's name have only the program id to show.
            if is_self && latest.label.as_deref().is_none_or(str::is_empty) {
                latest.label = Some(crate::SELF_NAME.to_string());
            }
            Entry {
                latest,
                installed: installed.get(&artifact.program).cloned(),
                is_self,
            }
        })
        .collect();
    let rank = |state: InstallState| match state {
        InstallState::UpdateAvailable => 0,
        InstallState::UpToDate => 1,
        InstallState::NotInstalled => 2,
    };
    entries.sort_by_cached_key(|e| {
        (
            rank(e.state()),
            e.latest.title().to_lowercase(),
            e.latest.program.clone(),
        )
    });
    entries
}

/// A program with a newer build in the registry than the one that is installed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Update {
    pub program: String,
    /// What a person sees: the label, else the program.
    pub title: String,
    pub version_name: String,
    pub version_code: i64,
}

/// What can be updated, by title. This app itself is included: it updates like any other.
#[must_use]
pub fn pending_updates(entries: &[Entry]) -> Vec<Update> {
    let mut updates: Vec<Update> = entries
        .iter()
        .filter(|e| e.state() == InstallState::UpdateAvailable)
        .map(|e| Update {
            program: e.latest.program.clone(),
            title: e.latest.title().to_string(),
            version_name: e.latest.version_name.clone(),
            version_code: e.latest.version_code,
        })
        .collect();
    updates.sort_by_cached_key(|u| (u.title.to_lowercase(), u.program.clone()));
    updates
}

/// Which updates there are, as one line: the same set gives the same line, a new version of any of them a different
/// one. What was announced is remembered by it, so a notification is sent when something new is there and not again
/// for what the user has already been told. Empty when there is nothing to update.
#[must_use]
pub fn announcement_key(updates: &[Update]) -> String {
    let mut parts: Vec<String> = updates
        .iter()
        .map(|u| format!("{}@{}", u.program, u.version_code))
        .collect();
    parts.sort();
    parts.join(",")
}
