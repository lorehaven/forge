//! What a runner Job does: an ordered list of steps, each one a small, closed set of operations.
//!
//! This is the whole interface between the service and the Job. It is data (JSON in the operation row and
//! in the Job's environment), never a script: the service cannot be made to run an arbitrary command
//! because there is no way to say one. Every value that reaches a command line is validated first
//! (`validate`) and then passed as its own argument, never through a shell.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The workload kinds a step may scale or wait on.
pub const WORKLOAD_KINDS: [&str; 3] = ["deployment", "statefulset", "daemonset"];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "step", rename_all = "snake_case")]
pub enum Step {
    /// Download a package version from Warehouse and verify its digest. Always first: everything after it
    /// runs from local files, so an operation never depends on a registry it may be restarting.
    Pull { package: String, version: String },
    /// Render the package with its variables and send it to the API server as a client-side dry run: proves
    /// every `${VAR}` it needs is supplied, with the cluster untouched.
    Check {
        package: String,
        version: String,
        #[serde(default)]
        values_secret: Option<String>,
    },
    /// Install the package: render, apply in dependency order, wait at the overlay's gates.
    Install {
        package: String,
        version: String,
        namespace: String,
        #[serde(default)]
        sets: BTreeMap<String, String>,
        /// `kind/name` -> replicas, applied on top of the package (how a stopped deployment stays stopped
        /// through an update).
        #[serde(default)]
        replicas: BTreeMap<String, u32>,
        #[serde(default)]
        values_secret: Option<String>,
        #[serde(default)]
        timeout_secs: Option<u64>,
        /// `kind/name` resources to apply, and nothing else of the package (how a single deployment is
        /// started: its workloads are re-rendered at the package's own replica counts).
        #[serde(default)]
        targets: Vec<String>,
        /// `kind/name` resources to leave out: Gantry's own Deployment is applied last, on its own.
        #[serde(default)]
        except: Vec<String>,
        /// Return once the manifests are accepted, without waiting for the rollout - for a step that a
        /// guarded `Rollout` follows, so a failure can be undone instead of just reported.
        #[serde(default)]
        no_wait: bool,
    },
    Scale {
        namespace: String,
        kind: String,
        name: String,
        replicas: u32,
    },
    /// Delete one resource, of any kind. How a workload is stopped, the Argo way: what the package
    /// declares stays known to Gantry, so it can be applied again from the package.
    Delete {
        /// As `kubectl` names it: `deployment.apps/sage`, `clusterrole.rbac.authorization.k8s.io/x`.
        resource: String,
        /// `None` for cluster-scoped kinds.
        #[serde(default)]
        namespace: Option<String>,
    },
    /// Apply edited YAML to the cluster as it is. What the package declares is not changed: the next install
    /// of the package overwrites this, and the resource is shown as edited until then.
    ApplyYaml {
        #[serde(default)]
        namespace: Option<String>,
        yaml: String,
    },
    /// Restart the workloads that use a ConfigMap or a Secret, so a changed value is actually picked up,
    /// and wait for them.
    RestartUsers {
        namespace: String,
        /// `configmap` or `secret`.
        kind: String,
        name: String,
        timeout_secs: u64,
    },
    /// Delete pods by label, for workloads nothing owns (Switchboard creates its vLLM pods directly).
    DeletePods {
        selector: String,
        #[serde(default)]
        namespaces: Vec<String>,
    },
    /// Block until no pod matches: a GPU is only free once the pods holding it are gone.
    WaitGone {
        selector: String,
        #[serde(default)]
        namespaces: Vec<String>,
        timeout_secs: u64,
    },
    /// Wait for a rollout; with `rollback_on_failure`, undo it if it does not become ready. A service that
    /// replaced itself with a broken pod cannot roll itself back, so the runner does.
    Rollout {
        namespace: String,
        kind: String,
        name: String,
        timeout_secs: u64,
        #[serde(default)]
        rollback_on_failure: bool,
    },
}

/// What the reconciler records once an operation has succeeded - never before, so a failed operation
/// leaves the recorded state describing what is actually running.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "effect", rename_all = "snake_case")]
pub enum Effect {
    /// A deployment is now meant to be `running` or `stopped`.
    SetDesired { deployment: String, desired: String },
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    pub steps: Vec<Step>,
    /// A few words for the history: "Upgrade media → 1.1.0", "Swap to training".
    #[serde(default)]
    pub title: String,
    /// Human-readable lines the planner wrote about what this does, shown before confirmation.
    #[serde(default)]
    pub summary: Vec<String>,
    /// Recorded when (and only when) the operation succeeds.
    #[serde(default)]
    pub effects: Vec<Effect>,
    /// Every package this touches: confirming needs permission on each.
    #[serde(default)]
    pub touches: Vec<String>,
    /// Deployments a start/stop/swap involves, for the stale check.
    #[serde(default)]
    pub deployments: Vec<String>,
}

impl Step {
    pub fn describe(&self) -> String {
        match self {
            Self::Pull { package, version } => format!("download {package} {version}"),
            Self::Check {
                package, version, ..
            } => format!("check {package} {version}"),
            Self::Install {
                package, version, ..
            } => format!("install {package} {version}"),
            Self::Scale {
                kind,
                name,
                replicas,
                ..
            } => format!("scale {kind}/{name} to {replicas}"),
            Self::Delete { resource, .. } => format!("delete {resource}"),
            Self::ApplyYaml { yaml, .. } => format!("apply edited {}", yaml_identity(yaml)),
            Self::RestartUsers { kind, name, .. } => format!("restart what uses {kind}/{name}"),
            Self::DeletePods { selector, .. } => format!("delete pods matching {selector}"),
            Self::WaitGone { selector, .. } => format!("wait for pods matching {selector} to go"),
            Self::Rollout {
                kind,
                name,
                rollback_on_failure,
                ..
            } => {
                if *rollback_on_failure {
                    format!("wait for {kind}/{name}, rolling back if it fails")
                } else {
                    format!("wait for {kind}/{name}")
                }
            }
        }
    }

    /// The Secrets (by name) this step reads variables from.
    pub fn values_secret(&self) -> Option<&str> {
        match self {
            Self::Check { values_secret, .. } | Self::Install { values_secret, .. } => {
                values_secret.as_deref()
            }
            _ => None,
        }
    }

    /// The same step without its values Secret, for when that Secret was not mounted (a package whose
    /// variables all have defaults needs none).
    #[must_use]
    pub fn without_values(&self) -> Self {
        let mut step = self.clone();
        if let Self::Check { values_secret, .. } | Self::Install { values_secret, .. } = &mut step {
            *values_secret = None;
        }
        step
    }

    /// Namespaces named explicitly; an empty list on a pod step means "wherever the allow-list says".
    pub fn namespaces(&self) -> Vec<&str> {
        match self {
            Self::Install { namespace, .. }
            | Self::Scale { namespace, .. }
            | Self::Rollout { namespace, .. }
            | Self::RestartUsers { namespace, .. } => vec![namespace.as_str()],
            Self::Delete { namespace, .. } | Self::ApplyYaml { namespace, .. } => {
                namespace.iter().map(String::as_str).collect()
            }
            Self::DeletePods { namespaces, .. } | Self::WaitGone { namespaces, .. } => {
                namespaces.iter().map(String::as_str).collect()
            }
            Self::Pull { .. } | Self::Check { .. } => Vec::new(),
        }
    }

    /// Whether this step takes a workload down (to zero), which is what the protected set forbids.
    pub fn stops(&self) -> Vec<(&str, &str)> {
        match self {
            Self::Scale {
                namespace,
                name,
                replicas: 0,
                ..
            } => vec![(namespace.as_str(), name.as_str())],
            Self::Delete {
                resource,
                namespace: Some(namespace),
            } => workload_name(resource)
                .map(|name| vec![(namespace.as_str(), name)])
                .unwrap_or_default(),
            Self::Install {
                namespace,
                replicas,
                ..
            } => replicas
                .iter()
                .filter(|(_, count)| **count == 0)
                .filter_map(|(resource, _)| resource.split_once('/'))
                .map(|(_, name)| (namespace.as_str(), name))
                .collect(),
            _ => Vec::new(),
        }
    }
}

impl Plan {
    /// Every Secret the plan reads variables from, once each.
    pub fn values_secrets(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .steps
            .iter()
            .filter_map(|step| step.values_secret().map(str::to_string))
            .collect();
        names.sort();
        names.dedup();
        names
    }

    /// Checks everything that ends up on a command line, and that every namespace named is one this
    /// Gantry may touch (`allowed` empty means no restriction).
    pub fn validate(&self, allowed: &[String]) -> Result<(), String> {
        if self.steps.is_empty() {
            return Err("a plan needs at least one step".to_string());
        }

        for (index, step) in self.steps.iter().enumerate() {
            validate_step(step, allowed)
                .map_err(|reason| format!("step {} ({}): {reason}", index + 1, step.describe()))?;
        }
        Ok(())
    }
}

fn validate_step(step: &Step, allowed: &[String]) -> Result<(), String> {
    for namespace in step.namespaces() {
        name(namespace, "namespace")?;
        if !allowed.is_empty() && !allowed.iter().any(|a| a == namespace) {
            return Err(format!(
                "namespace '{namespace}' is outside this Gantry's allow-list ({})",
                allowed.join(", ")
            ));
        }
    }

    match step {
        Step::Pull { package, version }
        | Step::Check {
            package, version, ..
        } => {
            name(package, "package")?;
            semver_like(version)?;
        }
        Step::Install {
            package,
            version,
            sets,
            replicas,
            targets,
            except,
            ..
        } => {
            name(package, "package")?;
            semver_like(version)?;
            for key in sets.keys() {
                variable(key)?;
            }
            for key in replicas.keys() {
                resource(key)?;
            }
            // Any kind may be applied or left out; only a replica count needs something that scales.
            for key in targets.iter().chain(except) {
                qualified_resource(key)?;
            }
        }
        Step::Scale { kind, name: n, .. } | Step::Rollout { kind, name: n, .. } => {
            workload_kind(kind)?;
            name(n, "name")?;
        }
        Step::DeletePods { selector, .. } | Step::WaitGone { selector, .. } => {
            label_selector(selector)?;
        }
        Step::Delete {
            resource,
            namespace,
        } => {
            qualified_resource(resource)?;
            // A namespaced kind is deleted in its namespace and a cluster-scoped one without; either way
            // the allow-list decides, so a step with no namespace is only allowed with no allow-list.
            if namespace.is_none() && !allowed.is_empty() {
                return Err(format!(
                    "{resource} has no namespace, and this Gantry may only act in {}",
                    allowed.join(", ")
                ));
            }
        }
        Step::ApplyYaml { namespace, yaml } => {
            if yaml.len() > MAX_YAML_BYTES {
                return Err(format!("the YAML is over {} KiB", MAX_YAML_BYTES / 1024));
            }
            let doc: serde_yaml::Value =
                serde_yaml::from_str(yaml).map_err(|e| format!("the YAML does not parse: {e}"))?;
            doc.get("kind")
                .and_then(serde_yaml::Value::as_str)
                .ok_or("the YAML has no kind")?;
            doc.get("metadata")
                .and_then(|m| m.get("name"))
                .and_then(serde_yaml::Value::as_str)
                .ok_or("the YAML has no metadata.name")?;
            if doc.get("kind").and_then(serde_yaml::Value::as_str) == Some("Secret") {
                return Err("a Secret is not edited here".to_string());
            }
            // What the YAML says must be what the step says: no editing one namespace through another.
            let declared = doc
                .get("metadata")
                .and_then(|m| m.get("namespace"))
                .and_then(serde_yaml::Value::as_str);
            if declared != namespace.as_deref() {
                return Err(format!(
                    "the YAML's namespace ({}) is not the one the step targets ({})",
                    declared.unwrap_or("none"),
                    namespace.as_deref().unwrap_or("none")
                ));
            }
            if namespace.is_none() && !allowed.is_empty() {
                return Err(format!(
                    "this resource has no namespace, and this Gantry may only act in {}",
                    allowed.join(", ")
                ));
            }
        }
        Step::RestartUsers { kind, name: n, .. } => {
            if !matches!(kind.as_str(), "configmap" | "secret") {
                return Err(format!("'{kind}' is not configmap or secret"));
            }
            name(n, "name")?;
        }
    }

    if let Some(secret) = step.values_secret() {
        name(secret, "values secret")?;
    }
    Ok(())
}

/// A DNS-1123 name: what Kubernetes itself will accept, and nothing that could read as a flag.
pub fn name(value: &str, what: &str) -> Result<(), String> {
    let ok = !value.is_empty()
        && value.len() <= 253
        && value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '.')
        && value.starts_with(|c: char| c.is_ascii_alphanumeric())
        && value.ends_with(|c: char| c.is_ascii_alphanumeric());
    if ok {
        Ok(())
    } else {
        Err(format!(
            "'{value}' is not a valid {what} (lowercase letters, digits, '-' and '.', starting and ending alphanumeric)"
        ))
    }
}

fn semver_like(value: &str) -> Result<(), String> {
    let ok = !value.is_empty()
        && value.len() <= 128
        && value.starts_with(|c: char| c.is_ascii_digit())
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '+' | '-'));
    if ok {
        Ok(())
    } else {
        Err(format!("'{value}' is not a version"))
    }
}

fn variable(value: &str) -> Result<(), String> {
    let ok = !value.is_empty()
        && value.starts_with(|c: char| c.is_ascii_uppercase() || c == '_')
        && value
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_');
    if ok {
        Ok(())
    } else {
        Err(format!("'{value}' is not a variable name (A-Z, 0-9, '_')"))
    }
}

fn workload_kind(value: &str) -> Result<(), String> {
    if WORKLOAD_KINDS.contains(&value) {
        Ok(())
    } else {
        Err(format!(
            "'{value}' is not a workload kind ({})",
            WORKLOAD_KINDS.join(", ")
        ))
    }
}

fn resource(value: &str) -> Result<(), String> {
    let (kind, n) = value
        .split_once('/')
        .ok_or_else(|| format!("'{value}' is not kind/name"))?;
    workload_kind(kind)?;
    name(n, "name")
}

/// The most YAML one edit may carry: it travels in the Job's environment.
pub const MAX_YAML_BYTES: usize = 128 * 1024;

/// `kind[.group]/name`, as kubectl takes it. Kind and group are DNS-ish; a name may carry `:` (RBAC).
fn qualified_resource(value: &str) -> Result<(), String> {
    let (kind, resource_name) = value
        .split_once('/')
        .ok_or_else(|| format!("'{value}' is not kind/name"))?;
    let kind_ok = !kind.is_empty()
        && kind.len() <= 253
        && kind.starts_with(|c: char| c.is_ascii_alphabetic())
        && kind
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
    let name_ok = !resource_name.is_empty()
        && resource_name.len() <= 253
        && resource_name.starts_with(|c: char| c.is_ascii_alphanumeric())
        && resource_name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':' | '_'));
    if kind_ok && name_ok {
        Ok(())
    } else {
        Err(format!("'{value}' is not a valid kind/name"))
    }
}

/// `deployment.apps/sage` -> `sage`, when it is a workload (what the protected set is about).
fn workload_name(resource: &str) -> Option<&str> {
    let (kind, name) = resource.split_once('/')?;
    let short = kind.split('.').next()?;
    WORKLOAD_KINDS.contains(&short).then_some(name)
}

/// `Deployment/name` from edited YAML, for a step's description.
pub fn yaml_identity(yaml: &str) -> String {
    serde_yaml::from_str::<serde_yaml::Value>(yaml)
        .ok()
        .and_then(|doc| {
            let kind = doc.get("kind")?.as_str()?.to_string();
            let name = doc.get("metadata")?.get("name")?.as_str()?.to_string();
            Some(format!("{kind}/{name}"))
        })
        .unwrap_or_else(|| "resource".to_string())
}

/// A label selector as `kubectl -l` takes it: `a=b,c!=d,e in (f,g)`. Restricted to the characters that
/// grammar uses, so it can never read as a flag or carry a second argument.
fn label_selector(value: &str) -> Result<(), String> {
    let ok = !value.is_empty()
        && value.len() <= 512
        && !value.starts_with('-')
        && value.chars().all(|c| {
            c.is_ascii_alphanumeric()
                || matches!(c, '=' | '!' | ',' | '.' | '/' | '_' | '-' | '(' | ')' | ' ')
        });
    if ok {
        Ok(())
    } else {
        Err(format!("'{value}' is not a label selector"))
    }
}
