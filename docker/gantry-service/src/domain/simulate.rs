//! A runner that changes the in-memory fake cluster instead of a real one, for trying Gantry with nothing
//! behind it: start, stop, swap and upgrade, and watch the state move. It does what the steps *mean* -
//! scale sets a count, an install puts a package's workloads at a version - and writes a log in the
//! runner's own style. It runs no `riveter` or `kubectl`, so it proves the planning, the confirming, the
//! recording and the screens, not the commands; `GANTRY_RUNNER=local` against a scratch namespace does that.

use crate::domain::cluster::{ClusterError, FakeCluster, JobStatus, LiveResource, Workload};
use crate::domain::executor::{Executor, Launch};
use crate::domain::registry::Registry;
use crate::domain::resources::InventoryItem;
use crate::domain::runner::RESULT_MARKER;
use crate::domain::steps::Step;
use async_trait::async_trait;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

pub struct SimulatedExecutor {
    cluster: Arc<FakeCluster>,
    registry: Arc<dyn Registry>,
    finished: Mutex<BTreeMap<String, (JobStatus, String)>>,
}

impl SimulatedExecutor {
    pub fn new(cluster: Arc<FakeCluster>, registry: Arc<dyn Registry>) -> Self {
        Self {
            cluster,
            registry,
            finished: Mutex::new(BTreeMap::new()),
        }
    }

    fn finished(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, (JobStatus, String)>> {
        self.finished
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Everything a package declares, as far as the simulation knows: what it was seeded with, what its
    /// manifest names, and whatever of it is already there.
    async fn package_items(
        &self,
        package: &str,
        version: &str,
    ) -> Result<Vec<InventoryItem>, String> {
        let known = self
            .registry
            .versions(package)
            .await
            .map_err(|e| e.to_string())?;
        let record = known
            .iter()
            .find(|v| v.version == version)
            .ok_or_else(|| format!("{package} {version} is not published"))?;

        let mut items = self.cluster.catalog(package);
        fn add_item(items: &mut Vec<InventoryItem>, item: InventoryItem) {
            let same = |other: &InventoryItem| {
                other.kind.eq_ignore_ascii_case(&item.kind)
                    && other.name == item.name
                    && other.namespace == item.namespace
            };
            if !items.iter().any(same) {
                items.push(item);
            }
        }
        let namespace = record.namespace.clone();
        for resource in record.deployments().into_iter().flat_map(|d| d.resources) {
            let (kind, name) = resource
                .split_once('/')
                .unwrap_or(("deployment", &resource));
            add_item(
                &mut items,
                InventoryItem {
                    api_version: Some("apps/v1".to_string()),
                    kind: capitalise(kind),
                    name: name.to_string(),
                    namespace: namespace.clone(),
                },
            );
        }
        for live in self
            .cluster
            .workloads()
            .iter()
            .filter(|w| w.package == package)
        {
            add_item(
                &mut items,
                InventoryItem {
                    api_version: Some("apps/v1".to_string()),
                    kind: capitalise(&live.kind),
                    name: live.name.clone(),
                    namespace: Some(live.namespace.clone()),
                },
            );
        }
        if items.is_empty() {
            add_item(
                &mut items,
                InventoryItem {
                    api_version: Some("apps/v1".to_string()),
                    kind: "Deployment".to_string(),
                    name: package.to_string(),
                    namespace,
                },
            );
        }
        Ok(items)
    }

    async fn apply(&self, step: &Step, log: &mut String) -> Result<(), String> {
        use std::fmt::Write;
        match step {
            Step::Pull { package, version } => {
                let _ = writeln!(log, "(simulated) downloaded {package} {version}");
            }
            Step::Check {
                package, version, ..
            } => {
                let items = self.package_items(package, version).await?;
                let _ = writeln!(log, "(simulated) {package}: every variable is supplied");
                let _ = writeln!(
                    log,
                    "riveter-inventory: {}",
                    serde_json::to_string(&items).unwrap_or_default()
                );
            }
            Step::Scale {
                namespace,
                kind,
                name,
                replicas,
            } => {
                let mut workloads = self.cluster.workloads();
                let found = workloads
                    .iter_mut()
                    .find(|w| &w.namespace == namespace && &w.kind == kind && &w.name == name)
                    .ok_or_else(|| format!("{kind}/{name} not found in {namespace}"))?;
                found.desired = i32::try_from(*replicas).unwrap_or(i32::MAX);
                found.ready = found.desired;
                self.cluster.upsert_workload(found.clone());
                let _ = writeln!(log, "{kind}.apps/{name} scaled to {replicas}");
            }
            Step::Delete {
                resource,
                namespace,
            } => {
                let (kind, name) = resource.split_once('/').unwrap_or(("", resource));
                let short = kind.split('.').next().unwrap_or(kind);
                if self
                    .cluster
                    .delete_resource(short, name, namespace.as_deref())
                {
                    let _ = writeln!(log, "{kind} \"{name}\" deleted");
                } else {
                    let _ = writeln!(log, "{resource} was not found (nothing to delete)");
                }
            }
            Step::ApplyYaml { yaml, .. } => {
                self.cluster.apply_yaml(yaml)?;
                let _ = writeln!(
                    log,
                    "{} configured",
                    crate::domain::steps::yaml_identity(yaml)
                );
            }
            Step::RestartUsers { kind, name, .. } => {
                let _ = writeln!(log, "(simulated) restarted what uses {kind}/{name}");
            }
            Step::DeletePods { selector, .. } => {
                let _ = writeln!(log, "pods matching {selector} deleted");
            }
            Step::WaitGone { selector, .. } => {
                let _ = writeln!(log, "none left matching {selector}");
            }
            Step::Rollout { kind, name, .. } => {
                let _ = writeln!(log, "{kind} \"{name}\" successfully rolled out");
            }
            Step::Install {
                package,
                version,
                namespace,
                replicas,
                targets,
                except,
                ..
            } => {
                let items = self.package_items(package, version).await?;
                let selected = |item: &InventoryItem| {
                    let key = format!("{}/{}", item.kind.to_ascii_lowercase(), item.name);
                    (targets.is_empty() || targets.iter().any(|t| t.eq_ignore_ascii_case(&key)))
                        && !except.iter().any(|e| e.eq_ignore_ascii_case(&key))
                };

                let mut applied = Vec::new();
                for item in items.iter().filter(|i| selected(i)) {
                    let ns = item.namespace.clone().or_else(|| Some(namespace.clone()));
                    let key = format!("{}/{}", item.kind.to_ascii_lowercase(), item.name);
                    if crate::domain::resources::is_workload(&item.kind) {
                        let desired = i32::try_from(replicas.get(&key).copied().unwrap_or(1))
                            .unwrap_or(i32::MAX);
                        self.cluster.upsert_workload(Workload {
                            kind: item.kind.to_ascii_lowercase(),
                            name: item.name.clone(),
                            namespace: ns.clone().unwrap_or_default(),
                            package: package.clone(),
                            version: Some(version.clone()),
                            desired,
                            ready: desired,
                        });
                    } else if !crate::domain::resources::is_secret(&item.kind) {
                        let yaml = format!(
                            "apiVersion: {}\nkind: {}\nmetadata:\n  name: {}\n{}",
                            item.api_version.clone().unwrap_or_else(|| "v1".to_string()),
                            item.kind,
                            item.name,
                            ns.as_ref()
                                .map_or(String::new(), |n| format!("  namespace: {n}\n"))
                        );
                        self.cluster.upsert_extra(crate::domain::cluster::Extra {
                            live: LiveResource {
                                api_version: item.api_version.clone(),
                                kind: item.kind.clone(),
                                name: item.name.clone(),
                                namespace: ns,
                                package: package.clone(),
                                version: Some(version.clone()),
                                ready: None,
                                edited: false,
                            },
                            yaml,
                        });
                    }
                    applied.push(key);
                }
                let _ = writeln!(
                    log,
                    "(simulated) applied {} resource(s) of {package} {version}: {}",
                    applied.len(),
                    applied.join(", ")
                );
                let _ = writeln!(
                    log,
                    "riveter-inventory: {}",
                    serde_json::to_string(&items).unwrap_or_default()
                );
            }
        }
        Ok(())
    }
}

fn capitalise(kind: &str) -> String {
    match kind.to_ascii_lowercase().as_str() {
        "deployment" => "Deployment".to_string(),
        "statefulset" => "StatefulSet".to_string(),
        "daemonset" => "DaemonSet".to_string(),
        other => {
            let mut chars = other.chars();
            chars.next().map_or(String::new(), |c| {
                c.to_uppercase().collect::<String>() + chars.as_str()
            })
        }
    }
}

#[async_trait]
impl Executor for SimulatedExecutor {
    async fn start(&self, launch: &Launch) -> Result<(), ClusterError> {
        if self.finished().contains_key(&launch.name) {
            return Ok(());
        }
        let total = launch.plan.steps.len();
        let mut log = String::new();
        let mut failure = None;
        for (index, step) in launch.plan.steps.iter().enumerate() {
            log.push_str(&format!(
                "== step {}/{total}: {}\n",
                index + 1,
                step.describe()
            ));
            if let Err(reason) = self.apply(step, &mut log).await {
                log.push_str(&format!("!! {reason}\n"));
                failure = Some(format!(
                    "step {} ({}): {reason}",
                    index + 1,
                    step.describe()
                ));
                break;
            }
        }
        let (status, marker) = match failure {
            None => (JobStatus::Succeeded, format!("{RESULT_MARKER} ok")),
            Some(reason) => (
                JobStatus::Failed(reason.clone()),
                format!("{RESULT_MARKER} failed {reason}"),
            ),
        };
        log.push_str(&marker);
        log.push('\n');
        self.finished().insert(launch.name.clone(), (status, log));
        Ok(())
    }

    async fn status(&self, name: &str) -> Result<JobStatus, ClusterError> {
        Ok(self
            .finished()
            .get(name)
            .map_or(JobStatus::Absent, |(status, _)| status.clone()))
    }

    async fn log(&self, name: &str) -> Result<String, ClusterError> {
        Ok(self
            .finished()
            .get(name)
            .map(|(_, log)| log.clone())
            .unwrap_or_default())
    }

    async fn stop(&self, _name: &str) -> Result<(), ClusterError> {
        Ok(())
    }

    async fn cleanup(&self, _name: &str) -> Result<(), ClusterError> {
        Ok(())
    }
}
