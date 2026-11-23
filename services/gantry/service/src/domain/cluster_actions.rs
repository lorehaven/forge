//! Things done to any object of the cluster, not only to what a package installed: delete it, scale it, apply
//! edited YAML.
//!
//! Gantry's own service account cannot change anything, by design. These are operations like every other: a
//! plan of one or two steps, run by a runner Job, with the log kept in the history.

use crate::domain::actions::ActionError;
use crate::domain::operation::Operation;
use crate::domain::resources::kubectl_name;
use crate::domain::service::Gantry;
use crate::domain::steps::{Plan, Step};

/// The most replicas a button sets: a typo is not a thousand pods.
pub const MAX_REPLICAS: u32 = 50;
const ROLLOUT_TIMEOUT_SECS: u64 = 300;

/// Kinds that are not deleted from here: they take data or a whole namespace with them, are read-only noise,
/// or are the machine itself.
const NOT_DELETED: [&str; 9] = [
    "namespace",
    "customresourcedefinition",
    "persistentvolume",
    "persistentvolumeclaim",
    "storageclass",
    "node",
    "event",
    "secret",
    "clusterrolebinding",
];

/// Which object.
#[derive(Clone, Debug, PartialEq, Eq, quench_serde::Deserialize)]
pub struct Object {
    pub kind: String,
    pub name: String,
    #[serde(default)]
    pub namespace: Option<String>,
}

fn plan(title: &str, steps: Vec<Step>, summary: Vec<String>) -> Plan {
    Plan {
        steps,
        title: title.to_string(),
        summary,
        ..Plan::default()
    }
}

fn label(object: &Object) -> String {
    format!("{}/{}", object.kind, object.name)
}

/// The `apiVersion` the cluster serves `kind` under, which `kubectl` needs to tell kinds of one name apart.
async fn api_version(gantry: &Gantry, kind: &str) -> Result<String, ActionError> {
    gantry
        .cluster
        .browse_kinds()
        .await?
        .into_iter()
        .find(|k| k.kind.eq_ignore_ascii_case(kind))
        .map(|k| k.api_version)
        .ok_or_else(|| ActionError::NotFound(format!("the cluster has no kind {kind}")))
}

/// Whether the object is there to act on.
async fn exists(gantry: &Gantry, object: &Object) -> Result<(), ActionError> {
    gantry
        .cluster
        .browse_object(&object.kind, object.namespace.as_deref(), &object.name)
        .await
        .map(|_| ())
        .map_err(|_| ActionError::NotFound(format!("{} is not there", label(object))))
}

pub async fn delete(gantry: &Gantry, object: &Object, by: &str) -> Result<Operation, ActionError> {
    if NOT_DELETED.contains(&object.kind.to_ascii_lowercase().as_str()) {
        return Err(ActionError::Refused(format!(
            "{} is not deleted from here",
            object.kind
        )));
    }
    exists(gantry, object).await?;
    let version = api_version(gantry, &object.kind).await?;
    let title = format!("Delete {}", label(object));
    let steps = vec![Step::Delete {
        resource: kubectl_name(Some(&version), &object.kind, &object.name),
        namespace: object.namespace.clone(),
    }];
    let summary = vec![format!("{title}.")];
    Ok(gantry
        .submit("delete", &title, plan(&title, steps, summary), by)
        .await?)
}

pub async fn scale(
    gantry: &Gantry,
    object: &Object,
    replicas: u32,
    by: &str,
) -> Result<Operation, ActionError> {
    let kind = object.kind.to_ascii_lowercase();
    if !matches!(kind.as_str(), "deployment" | "statefulset") {
        return Err(ActionError::Refused(format!(
            "{} cannot be scaled",
            object.kind
        )));
    }
    if replicas > MAX_REPLICAS {
        return Err(ActionError::Refused(format!(
            "{replicas} replicas is more than the {MAX_REPLICAS} a button sets"
        )));
    }
    let namespace = object
        .namespace
        .clone()
        .ok_or_else(|| ActionError::Refused("a workload lives in a namespace".to_string()))?;
    exists(gantry, object).await?;
    let title = format!("Scale {} to {replicas}", label(object));
    let steps = vec![
        Step::Scale {
            namespace: namespace.clone(),
            kind: kind.clone(),
            name: object.name.clone(),
            replicas,
        },
        Step::Rollout {
            namespace,
            kind,
            name: object.name.clone(),
            timeout_secs: ROLLOUT_TIMEOUT_SECS,
            rollback_on_failure: false,
        },
    ];
    let summary = vec![format!("{title}, and wait for it.")];
    Ok(gantry
        .submit("scale", &title, plan(&title, steps, summary), by)
        .await?)
}

/// Apply YAML as the person edited it. It must describe the object they opened: this edits one thing.
pub async fn edit(
    gantry: &Gantry,
    object: &Object,
    yaml: &str,
    by: &str,
) -> Result<Operation, ActionError> {
    if object.kind.eq_ignore_ascii_case("secret") {
        return Err(ActionError::Refused(
            "a Secret is not edited here".to_string(),
        ));
    }
    let doc: quench_serde::Value = quench_serde::yaml::from_str(yaml)
        .map_err(|e| ActionError::Refused(format!("the YAML does not parse: {e}")))?;
    let text = |value: Option<&quench_serde::Value>| {
        value
            .and_then(quench_serde::Value::as_str)
            .map(str::to_string)
    };
    let kind = text(doc.get("kind")).unwrap_or_default();
    let name = text(doc.get("metadata").and_then(|m| m.get("name"))).unwrap_or_default();
    let namespace = text(doc.get("metadata").and_then(|m| m.get("namespace")));
    if !kind.eq_ignore_ascii_case(&object.kind)
        || name != object.name
        || namespace != object.namespace
    {
        return Err(ActionError::Refused(format!(
            "this edits {}; the YAML describes {kind}/{name}. Edit the one you opened, not another",
            label(object)
        )));
    }
    exists(gantry, object).await?;

    let title = format!("Edit {}", label(object));
    let mut steps = vec![Step::ApplyYaml {
        namespace: object.namespace.clone(),
        yaml: yaml.to_string(),
    }];
    let mut summary = vec![format!("Apply the edited {} as it is.", label(object))];
    let lower = object.kind.to_ascii_lowercase();
    if let (true, Some(ns)) = (
        matches!(lower.as_str(), "deployment" | "statefulset" | "daemonset"),
        object.namespace.clone(),
    ) {
        steps.push(Step::Rollout {
            namespace: ns,
            kind: lower,
            name: object.name.clone(),
            timeout_secs: ROLLOUT_TIMEOUT_SECS,
            rollback_on_failure: false,
        });
        summary.push("Wait for the rollout this causes.".to_string());
    }
    Ok(gantry
        .submit("edit", &title, plan(&title, steps, summary), by)
        .await?)
}
