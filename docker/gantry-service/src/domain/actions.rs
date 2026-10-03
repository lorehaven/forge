//! Things done to one resource, directly: delete it, apply it again from the package, edit its YAML.
//!
//! There is no plan to confirm. Each is small, says what it does on the button, and the operation it
//! creates (with its log) is the record. They are the Argo-style way of stopping and starting: *stop* is
//! delete, *start* is apply from the package - and Gantry can apply it because it remembers what the
//! package declares (the inventory), even for something that is no longer in the cluster.

use crate::domain::cluster::EDITED_ANNOTATION;
use crate::domain::operation::Operation;
use crate::domain::resources::{self, Group, Row, State, kubectl_name};
use crate::domain::service::{Gantry, SubmitError};
use crate::domain::steps::{Plan, Step};

const INSTALL_TIMEOUT_SECS: u64 = 600;
const ROLLOUT_TIMEOUT_SECS: u64 = 300;

#[derive(Debug, thiserror::Error)]
pub enum ActionError {
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Refused(String),
    #[error(transparent)]
    Submit(#[from] SubmitError),
    #[error(transparent)]
    Planner(#[from] crate::domain::planner::PlanError),
    #[error(transparent)]
    Store(#[from] crate::domain::GantryError),
    #[error(transparent)]
    Cluster(#[from] crate::domain::cluster::ClusterError),
}

/// Which resource: how the lists name it, so a button can carry it.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Target {
    #[serde(rename = "apiVersion", default)]
    pub api_version: Option<String>,
    pub kind: String,
    pub name: String,
    #[serde(default)]
    pub namespace: Option<String>,
}

/// Kinds a delete would take data or a whole namespace with it. Not a button's worth of decision.
const NOT_DELETED_HERE: [&str; 5] = [
    "namespace",
    "customresourcedefinition",
    "persistentvolume",
    "persistentvolumeclaim",
    "storageclass",
];

/// Every resource of every package, grouped, with what is missing and what has been edited.
pub async fn groups(gantry: &Gantry) -> Result<Vec<Group>, ActionError> {
    let targets = crate::domain::planner::all(gantry).await?;
    let live = gantry
        .cluster
        .list_resources(&gantry.settings.allowed_namespaces)
        .await?;
    let inventories = gantry.store.all_inventories().await?;
    Ok(resources::group(&targets, &live, &inventories))
}

async fn find(
    gantry: &Gantry,
    package: &str,
    target: &Target,
) -> Result<(Group, Row), ActionError> {
    let group = groups(gantry)
        .await?
        .into_iter()
        .find(|g| g.package == package)
        .ok_or_else(|| ActionError::NotFound(format!("no package named '{package}'")))?;
    let row = group
        .rows
        .iter()
        .find(|row| {
            row.kind.eq_ignore_ascii_case(&target.kind)
                && row.name == target.name
                && row.namespace == target.namespace
        })
        .cloned()
        .ok_or_else(|| {
            ActionError::NotFound(format!(
                "{}/{} is not part of {package}",
                target.kind, target.name
            ))
        })?;
    Ok((group, row))
}

fn label(row: &Row) -> String {
    format!("{}/{}", row.kind, row.name)
}

fn plan(title: String, steps: Vec<Step>, summary: Vec<String>, package: &str) -> Plan {
    Plan {
        steps,
        title,
        summary,
        touches: vec![package.to_string()],
        ..Plan::default()
    }
}

/// Delete one resource. Protected workloads are refused by `submit`, whatever asks.
pub async fn delete(
    gantry: &Gantry,
    package: &str,
    target: &Target,
    by: &str,
) -> Result<Operation, ActionError> {
    let (_, row) = find(gantry, package, target).await?;
    if NOT_DELETED_HERE.contains(&row.kind.to_ascii_lowercase().as_str()) {
        return Err(ActionError::Refused(format!(
            "{} is not deleted from here: it would take data or a whole namespace with it",
            label(&row)
        )));
    }
    if row.state == State::Missing {
        return Err(ActionError::Refused(format!(
            "{} is already gone",
            label(&row)
        )));
    }
    if row.state == State::Hidden {
        return Err(ActionError::Refused(
            "a Secret is not deleted from here".to_string(),
        ));
    }

    let title = format!("Delete {}", label(&row));
    let plan = plan(
        title.clone(),
        vec![Step::Delete {
            resource: kubectl_name(row.api_version.as_deref(), &row.kind, &row.name),
            namespace: row.namespace.clone(),
        }],
        vec![format!("{title}.")],
        package,
    );
    Ok(gantry.submit("delete", &title, plan, by).await?)
}

/// Apply resources of a package again from the package, as it was last installed. With no `only`, everything
/// the package declares that is missing.
pub async fn apply(
    gantry: &Gantry,
    package: &str,
    only: Option<&Target>,
    by: &str,
) -> Result<Operation, ActionError> {
    let groups = groups(gantry).await?;
    let group = groups
        .iter()
        .find(|g| g.package == package)
        .ok_or_else(|| ActionError::NotFound(format!("no package named '{package}'")))?;

    let version = group.installed.clone().ok_or_else(|| {
        ActionError::Refused(format!(
            "{package} has no recorded install to apply from; install it first"
        ))
    })?;

    let wanted: Vec<&Row> = match only {
        Some(target) => group
            .rows
            .iter()
            .filter(|row| {
                row.kind.eq_ignore_ascii_case(&target.kind)
                    && row.name == target.name
                    && row.namespace == target.namespace
            })
            .collect(),
        None => group
            .rows
            .iter()
            .filter(|row| row.state == State::Missing)
            .collect(),
    };
    if wanted.is_empty() {
        return Err(ActionError::Refused(match only {
            Some(_) => "that is not part of the package".to_string(),
            None => format!("nothing of {package} is missing"),
        }));
    }
    if group.rows.iter().all(|r| r.state == State::Extra) {
        return Err(ActionError::Refused(format!(
            "{package} has no inventory yet, so it cannot tell what to apply"
        )));
    }

    let targets: Vec<String> = wanted
        .iter()
        .map(|row| format!("{}/{}", row.kind.to_ascii_lowercase(), row.name))
        .collect();
    let namespace = wanted
        .iter()
        .find_map(|row| row.namespace.clone())
        .or_else(|| group.rows.iter().find_map(|row| row.namespace.clone()))
        .ok_or_else(|| ActionError::Refused(format!("{package} declares no namespace")))?;

    let title = match only {
        Some(_) => format!("Apply {}", targets[0]),
        None => format!("Apply {} missing from {package}", wanted.len()),
    };
    let steps = vec![
        Step::Pull {
            package: package.to_string(),
            version: version.clone(),
        },
        Step::Check {
            package: package.to_string(),
            version: version.clone(),
            values_secret: None,
        },
        Step::Install {
            package: package.to_string(),
            version: version.clone(),
            namespace,
            sets: std::collections::BTreeMap::new(),
            replicas: std::collections::BTreeMap::new(),
            values_secret: None,
            timeout_secs: Some(INSTALL_TIMEOUT_SECS),
            targets: targets.clone(),
            except: Vec::new(),
            no_wait: false,
        },
    ];
    let plan = plan(
        title.clone(),
        steps,
        vec![format!("{title}, from {package} {version}.")],
        package,
    );
    Ok(gantry.submit("apply", &title, plan, by).await?)
}

/// The current YAML of a resource, for editing.
pub async fn yaml(gantry: &Gantry, package: &str, target: &Target) -> Result<String, ActionError> {
    let (_, row) = find(gantry, package, target).await?;
    if row.state == State::Hidden {
        return Err(ActionError::Refused(
            "a Secret is never shown; its values are set through the package's values Secret"
                .to_string(),
        ));
    }
    if row.state == State::Missing {
        return Err(ActionError::Refused(format!(
            "{} is not in the cluster; apply it first",
            label(&row)
        )));
    }
    Ok(gantry
        .cluster
        .resource_yaml(
            row.api_version.as_deref(),
            &row.kind,
            row.namespace.as_deref(),
            &row.name,
        )
        .await?)
}

/// Apply edited YAML as it is. What the package declares is untouched; the next install of the package
/// overwrites the edit, and until then the resource is marked edited. A workload is waited on; what
/// reads a ConfigMap is restarted so the change is picked up.
pub async fn edit(
    gantry: &Gantry,
    package: &str,
    target: &Target,
    yaml: &str,
    by: &str,
) -> Result<Operation, ActionError> {
    let (_, row) = find(gantry, package, target).await?;
    if row.state == State::Hidden {
        return Err(ActionError::Refused(
            "a Secret is not edited here".to_string(),
        ));
    }

    let mut doc: serde_yaml::Value = serde_yaml::from_str(yaml)
        .map_err(|e| ActionError::Refused(format!("the YAML does not parse: {e}")))?;
    let text = |value: Option<&serde_yaml::Value>| {
        value
            .and_then(serde_yaml::Value::as_str)
            .map(str::to_string)
    };
    let kind = text(doc.get("kind")).unwrap_or_default();
    let name = text(doc.get("metadata").and_then(|m| m.get("name"))).unwrap_or_default();
    let namespace = text(doc.get("metadata").and_then(|m| m.get("namespace")));
    if !kind.eq_ignore_ascii_case(&row.kind) || name != row.name || namespace != row.namespace {
        return Err(ActionError::Refused(format!(
            "this edits {}; the YAML describes {kind}/{name}. Edit the one you opened, not another",
            label(&row)
        )));
    }

    // Say, on the object itself, that it was changed from here.
    let marker = format!(
        "{by} at {}",
        chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
    );
    let metadata = doc
        .get_mut("metadata")
        .and_then(serde_yaml::Value::as_mapping_mut)
        .ok_or_else(|| ActionError::Refused("the YAML has no metadata".to_string()))?;
    let annotations = metadata
        .entry(serde_yaml::Value::String("annotations".to_string()))
        .or_insert_with(|| serde_yaml::Value::Mapping(serde_yaml::Mapping::new()));
    if let Some(annotations) = annotations.as_mapping_mut() {
        annotations.insert(
            serde_yaml::Value::String(EDITED_ANNOTATION.to_string()),
            serde_yaml::Value::String(marker),
        );
    }
    let yaml = serde_yaml::to_string(&doc).map_err(|e| ActionError::Refused(e.to_string()))?;

    let title = format!("Edit {}", label(&row));
    let mut steps = vec![Step::ApplyYaml {
        namespace: row.namespace.clone(),
        yaml,
    }];
    let mut summary = vec![format!("Apply the edited {} as it is.", label(&row))];

    let lower = row.kind.to_ascii_lowercase();
    if let (true, Some(ns)) = (row.workload, row.namespace.clone()) {
        steps.push(Step::Rollout {
            namespace: ns,
            kind: lower,
            name: row.name.clone(),
            timeout_secs: ROLLOUT_TIMEOUT_SECS,
            rollback_on_failure: false,
        });
        summary.push("Wait for the rollout this causes.".to_string());
    } else if let (true, Some(ns)) = (matches!(lower.as_str(), "configmap"), row.namespace.clone())
    {
        steps.push(Step::RestartUsers {
            namespace: ns,
            kind: lower,
            name: row.name.clone(),
            timeout_secs: ROLLOUT_TIMEOUT_SECS,
        });
        summary.push("Restart what uses it, so the change is picked up.".to_string());
    }
    summary.push(format!(
        "The next install of {package} overwrites this edit; until then it is shown as edited."
    ));

    let plan = plan(title.clone(), steps, summary, package);
    Ok(gantry.submit("edit", &title, plan, by).await?)
}

/// Learn what a package declares without changing anything: render it at the installed version and keep
/// the list. For packages installed before Gantry kept inventories, and after an upgrade by other means.
pub async fn refresh(gantry: &Gantry, package: &str, by: &str) -> Result<Operation, ActionError> {
    let group = groups(gantry)
        .await?
        .into_iter()
        .find(|g| g.package == package)
        .ok_or_else(|| ActionError::NotFound(format!("no package named '{package}'")))?;
    let version = group.installed.ok_or_else(|| {
        ActionError::Refused(format!(
            "{package} is not installed, so there is nothing to look at"
        ))
    })?;

    let title = format!("Refresh {package}");
    let plan = plan(
        title.clone(),
        vec![
            Step::Pull {
                package: package.to_string(),
                version: version.clone(),
            },
            Step::Check {
                package: package.to_string(),
                version: version.clone(),
                values_secret: None,
            },
        ],
        vec![format!(
            "Read what {package} {version} declares. Nothing is changed."
        )],
        package,
    );
    Ok(gantry.submit("refresh", &title, plan, by).await?)
}
