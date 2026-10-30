//! The JSON the API speaks. Timestamps stay RFC 3339 text: the browser only shows them.

use quench_serde::{Deserialize, Serialize};

/// The states a package can be in against what it declares, in the order the home filter offers them.
pub const SYNC_STATES: [&str; 5] = [
    "synced",
    "out_of_sync",
    "missing",
    "not_installed",
    "unlisted",
];
/// The states one resource can be in, in the order the filter offers them.
pub const RESOURCE_STATES: [&str; 5] = ["synced", "edited", "missing", "extra", "hidden"];

/// Who is asking, and what they may do anywhere (`GET /api/v1/info`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Info {
    #[serde(default)]
    pub actor: String,
    #[serde(default)]
    pub version: String,
    /// Of `read`, `deploy`, `scale`, `activate`, `rollback`: the ones held for every package.
    #[serde(default)]
    pub can: Vec<String>,
}

impl Info {
    #[must_use]
    pub fn may(&self, action: &str) -> bool {
        self.can.iter().any(|held| held == action)
    }
}

/// What a card or a header says about one application, counted by the service from its rows.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Summary {
    /// `synced`, `out_of_sync`, `missing`, `not_installed` or `unlisted`.
    pub sync: String,
    pub total: usize,
    pub synced: usize,
    pub edited: usize,
    pub missing: usize,
    pub extra: usize,
    #[serde(default)]
    pub workloads: usize,
    pub not_ready: usize,
    /// The newer version the package offers, when the running one is behind.
    #[serde(default)]
    pub update_to: Option<String>,
}

/// One resource of a package, with its state against the package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Row {
    #[serde(rename = "apiVersion", default)]
    pub api_version: Option<String>,
    pub kind: String,
    pub name: String,
    #[serde(default)]
    pub namespace: Option<String>,
    /// `synced`, `edited`, `missing`, `extra` or `hidden`.
    pub state: String,
    /// `1/1` for a workload that reports it.
    #[serde(default)]
    pub ready: Option<String>,
    #[serde(default)]
    pub workload: bool,
    #[serde(default)]
    pub editable: bool,
}

/// A package and what it put in the cluster.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Group {
    pub package: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub installed: Option<String>,
    #[serde(default)]
    pub offered: Option<String>,
    #[serde(default)]
    pub status: String,
    /// Whether an inventory exists, i.e. whether missing resources can be known about at all.
    #[serde(default)]
    pub inventoried: bool,
    #[serde(default)]
    pub rows: Vec<Row>,
    pub summary: Summary,
    /// Of `read`, `deploy`, `scale`: what the caller may do to this package.
    #[serde(default)]
    pub can: Vec<String>,
}

impl Group {
    #[must_use]
    pub fn may(&self, action: &str) -> bool {
        self.can.iter().any(|held| held == action)
    }

    /// The kinds present, for a filter.
    #[must_use]
    pub fn kinds(&self) -> Vec<String> {
        let mut kinds: Vec<String> = self.rows.iter().map(|r| r.kind.clone()).collect();
        kinds.sort_unstable();
        kinds.dedup();
        kinds
    }

    /// The rows that pass a filter: an empty `kind` or `state` matches everything, and `query` is a
    /// case-insensitive piece of the name.
    #[must_use]
    pub fn filtered(&self, kind: &str, state: &str, query: &str) -> Vec<Row> {
        let query = query.trim().to_ascii_lowercase();
        self.rows
            .iter()
            .filter(|r| kind.is_empty() || r.kind.eq_ignore_ascii_case(kind))
            .filter(|r| state.is_empty() || r.state == state)
            .filter(|r| query.is_empty() || r.name.to_ascii_lowercase().contains(&query))
            .cloned()
            .collect()
    }

    /// The version shown under the name: the one running, and the newer one on offer.
    #[must_use]
    pub fn versions(&self) -> String {
        let installed = self.installed.clone().unwrap_or_else(|| "—".to_string());
        match &self.summary.update_to {
            Some(latest) => format!("{installed} → {latest}"),
            None => installed,
        }
    }
}

/// One start/stop-able thing a package declares.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeploymentView {
    pub name: String,
    #[serde(default)]
    pub declared: bool,
    /// The package it belongs to.
    pub target: String,
    #[serde(default)]
    pub description: Option<String>,
    /// `running`, `stopped`, `partial` or `absent`.
    pub observed: String,
    #[serde(default)]
    pub conflicts_with: Vec<String>,
}

impl DeploymentView {
    #[must_use]
    pub fn installed(&self) -> bool {
        self.observed != "absent"
    }

    /// Whether Stop is offered: something of it is up.
    #[must_use]
    pub fn can_stop(&self) -> bool {
        matches!(self.observed.as_str(), "running" | "partial")
    }

    /// Whether Start is offered: installed, and not all of it up.
    #[must_use]
    pub fn can_start(&self) -> bool {
        self.installed() && self.observed != "running"
    }
}

/// A workload a package has running.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Unit {
    pub kind: String,
    pub name: String,
    pub namespace: String,
    #[serde(default)]
    pub version: Option<String>,
    pub desired: i32,
    pub ready: i32,
}

/// One published version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageVersion {
    #[serde(default)]
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub yanked: bool,
}

/// A package: what runs of it, what is published, and the versions (`GET /api/v1/targets/{name}`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetView {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub installed: Option<String>,
    #[serde(default)]
    pub offered: Option<String>,
    /// `not_installed`, `current`, `update_available`, `ahead`, `mixed` or `unlisted`.
    pub status: String,
    #[serde(default)]
    pub units: Vec<Unit>,
    #[serde(default)]
    pub versions: Vec<PackageVersion>,
}

/// What a plan does, in words (the mechanical steps are `step_lines` of the operation or stored plan).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub summary: Vec<String>,
    #[serde(default)]
    pub touches: Vec<String>,
    #[serde(default)]
    pub deployments: Vec<String>,
}

/// One requested change and how it went.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Operation {
    pub id: String,
    #[serde(default)]
    pub kind: String,
    pub title: String,
    /// `queued`, `running`, `succeeded`, `failed`, `rolled_back` or `cancelled`.
    pub state: String,
    #[serde(default)]
    pub requested_by: String,
    #[serde(default)]
    pub error: Option<String>,
    /// Written when the operation ends.
    #[serde(default)]
    pub log: Option<String>,
    /// The runner's output so far, while it runs.
    #[serde(default)]
    pub live_log: Option<String>,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub plan: Plan,
    #[serde(default)]
    pub step_lines: Vec<String>,
}

impl Operation {
    /// Over: it will not change any more.
    #[must_use]
    pub fn is_terminal(&self) -> bool {
        !matches!(self.state.as_str(), "queued" | "running")
    }

    /// While it runs the log is the runner's own output; afterwards it is the copy kept at the end.
    #[must_use]
    pub fn log_text(&self) -> String {
        let text = if self.is_terminal() {
            self.log.clone()
        } else {
            self.live_log.clone()
        };
        text.unwrap_or_default()
    }
}

/// A change in progress, named on the home page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Busy {
    pub id: String,
    pub title: String,
}

/// A plan made and kept for someone to look at and confirm (`GET /api/v1/plans/{id}`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredPlan {
    pub id: String,
    pub target: String,
    pub action: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub plan: Plan,
    /// Set once confirmed: a plan runs at most once. `claiming` while it is being confirmed.
    #[serde(default)]
    pub operation_id: Option<String>,
    #[serde(default)]
    pub step_lines: Vec<String>,
    #[serde(default)]
    pub can_confirm: bool,
}

impl StoredPlan {
    /// The operation it became, once there is one.
    #[must_use]
    pub fn confirmed(&self) -> Option<&str> {
        self.operation_id.as_deref().filter(|id| *id != "claiming")
    }

    /// A start, stop or swap belongs under the packages; everything else is a change to one package.
    #[must_use]
    pub fn about_deployments(&self) -> bool {
        !self.plan.deployments.is_empty()
    }
}

/// The YAML of a live resource (`GET /api/v1/resources/yaml`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceYaml {
    pub yaml: String,
    /// Whether the caller may change it.
    #[serde(default)]
    pub editable: bool,
}

/// Which resource of which package: what the buttons and the editor's address carry.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResourceRef {
    pub package: String,
    pub api_version: String,
    pub kind: String,
    pub name: String,
    pub namespace: String,
}

fn optional(text: &str) -> Option<String> {
    (!text.is_empty()).then(|| text.to_string())
}

impl ResourceRef {
    /// The resource a table row stands for.
    #[must_use]
    pub fn of(package: &str, row: &Row) -> Self {
        Self {
            package: package.to_string(),
            api_version: row.api_version.clone().unwrap_or_default(),
            kind: row.kind.clone(),
            name: row.name.clone(),
            namespace: row.namespace.clone().unwrap_or_default(),
        }
    }

    /// `package=ml&kind=ConfigMap&...`, percent-encoded, empty parts left out: the editor's address.
    #[must_use]
    pub fn query(&self) -> String {
        [
            ("package", &self.package),
            ("kind", &self.kind),
            ("name", &self.name),
            ("namespace", &self.namespace),
            ("api_version", &self.api_version),
        ]
        .into_iter()
        .filter(|(_, value)| !value.is_empty())
        .map(|(key, value)| format!("{key}={}", crate::format::encode(value)))
        .collect::<Vec<_>>()
        .join("&")
    }

    /// The reverse of [`query`](Self::query), from the decoded pairs of an address. `None` without a package, a
    /// kind and a name.
    #[must_use]
    pub fn from_pairs(pairs: &[(String, String)]) -> Option<Self> {
        let get = |name: &str| {
            pairs
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.clone())
                .unwrap_or_default()
        };
        let found = Self {
            package: get("package"),
            api_version: get("api_version"),
            kind: get("kind"),
            name: get("name"),
            namespace: get("namespace"),
        };
        (!found.package.is_empty() && !found.kind.is_empty() && !found.name.is_empty())
            .then_some(found)
    }

    /// `Deployment/sage`.
    #[must_use]
    pub fn title(&self) -> String {
        format!("{}/{}", self.kind, self.name)
    }

    #[must_use]
    pub fn body(&self) -> ResourceBody {
        ResourceBody {
            package: self.package.clone(),
            api_version: optional(&self.api_version),
            kind: self.kind.clone(),
            name: self.name.clone(),
            namespace: optional(&self.namespace),
        }
    }

    #[must_use]
    pub fn edit_body(&self, yaml: String) -> EditBody {
        EditBody {
            package: self.package.clone(),
            api_version: optional(&self.api_version),
            kind: self.kind.clone(),
            name: self.name.clone(),
            namespace: optional(&self.namespace),
            yaml,
        }
    }
}

/// `POST /api/v1/resources/{delete,apply}` for one resource.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceBody {
    pub package: String,
    #[serde(rename = "apiVersion")]
    pub api_version: Option<String>,
    pub kind: String,
    pub name: String,
    pub namespace: Option<String>,
}

/// `POST /api/v1/resources/edit`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditBody {
    pub package: String,
    #[serde(rename = "apiVersion")]
    pub api_version: Option<String>,
    pub kind: String,
    pub name: String,
    pub namespace: Option<String>,
    pub yaml: String,
}

/// `POST /api/v1/resources/{apply,refresh}` for a whole package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageBody {
    pub package: String,
}

/// `POST /api/v1/targets/{name}/plan`: the version to put the package at, the newest when none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanRequest {
    pub version: Option<String>,
}

/// `POST /api/v1/deployments/{name}/plan`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeploymentPlan {
    /// `start` or `stop`.
    pub action: String,
    pub stop: Vec<String>,
}
