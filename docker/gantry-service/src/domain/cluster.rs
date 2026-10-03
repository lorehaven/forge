//! What Gantry asks of the cluster. A trait, so the reconciler and the planner are tested against a fake
//! and a developer can run the whole service with no cluster at all.

use crate::domain::settings::{ClusterMode, Settings};
use async_trait::async_trait;
use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum ClusterError {
    #[error("kubernetes: {0}")]
    Kube(String),
}

/// A runner Job, before it is a manifest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobSpec {
    pub namespace: String,
    pub name: String,
    pub image: String,
    pub service_account: String,
    /// The plan as JSON: step data only, never a value from a Secret.
    pub plan_json: String,
    pub allowed_namespaces: Vec<String>,
    pub credentials_secret: String,
    /// The Secret holding the age key (`age.key`) that opens a package's encrypted values.
    pub age_key_secret: String,
    /// `gantry-values-<package>` Secrets to mount; the Job reads them, the service never does.
    pub values_secrets: Vec<String>,
    pub operation_id: String,
    pub deadline_secs: u64,
}

/// Anything a package put in the cluster, of any kind, as Gantry sees it.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LiveResource {
    #[serde(rename = "apiVersion", default)]
    pub api_version: Option<String>,
    pub kind: String,
    pub name: String,
    /// `None` for cluster-scoped kinds.
    #[serde(default)]
    pub namespace: Option<String>,
    pub package: String,
    #[serde(default)]
    pub version: Option<String>,
    /// `1/1`, for what reports it.
    #[serde(default)]
    pub ready: Option<String>,
    /// Changed through Gantry since the package was installed (the `gantry.forge/edited` annotation).
    #[serde(default)]
    pub edited: bool,
    /// Another object owns it (an `ownerReference`): something Kubernetes or a controller made from what
    /// the package installed - cert-manager's Certificate for an Ingress, which copies the Ingress's labels
    /// onto it. Not the package's own, so not listed unless the package declares it.
    #[serde(default)]
    pub owned: bool,
}

/// A workload (Deployment, StatefulSet or DaemonSet) a package put in the cluster, as Gantry sees it.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Workload {
    pub kind: String,
    pub name: String,
    pub namespace: String,
    /// Which package installed it: the `riveter.forge/package` label.
    pub package: String,
    /// Which version of it: the `riveter.forge/package-version` annotation, if Riveter stamped one.
    pub version: Option<String>,
    pub desired: i32,
    pub ready: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JobStatus {
    /// No Job by that name.
    Absent,
    Active,
    Succeeded,
    Failed(String),
}

impl JobSpec {
    /// The Job as the API server takes it.
    pub fn manifest(&self) -> serde_json::Value {
        let mut volumes = vec![serde_json::json!({"name": "work", "emptyDir": {}})];
        let mut mounts = vec![serde_json::json!({"name": "work", "mountPath": "/work"})];
        for secret in &self.values_secrets {
            volumes.push(serde_json::json!({
                "name": format!("values-{secret}"),
                // Optional: a package whose variables all have defaults has no values Secret.
                "secret": {"secretName": secret, "optional": true},
            }));
            mounts.push(serde_json::json!({
                "name": format!("values-{secret}"),
                "mountPath": format!("/values/{secret}"),
                "readOnly": true,
            }));
        }

        // The key that opens a package's encrypted values, as a file riveter finds by `RIVETER_AGE_KEY_FILE`.
        // Optional, like the values Secrets: a package with no encrypted values needs no key, and the Job
        // must not fail to start because there is none.
        volumes.push(serde_json::json!({
            "name": "age-key",
            "secret": {"secretName": self.age_key_secret, "optional": true},
        }));
        mounts.push(serde_json::json!({"name": "age-key", "mountPath": "/keys", "readOnly": true}));

        let labels = BTreeMap::from([
            ("app.kubernetes.io/managed-by", "gantry".to_string()),
            ("app.kubernetes.io/name", "gantry-runner".to_string()),
            ("gantry.forge/operation", self.operation_id.clone()),
        ]);

        serde_json::json!({
            "apiVersion": "batch/v1",
            "kind": "Job",
            "metadata": {
                "name": self.name,
                "namespace": self.namespace,
                "labels": labels,
            },
            "spec": {
                // A failed step is reported, not retried: the service decides what happens next.
                "backoffLimit": 0,
                "activeDeadlineSeconds": self.deadline_secs,
                // Long enough to read the log of yesterday's upgrade; the service stores a copy anyway.
                "ttlSecondsAfterFinished": 86400,
                "template": {
                    "metadata": {"labels": labels},
                    "spec": {
                        "restartPolicy": "Never",
                        "serviceAccountName": self.service_account,
                        "containers": [{
                            "name": "runner",
                            "image": self.image,
                            "imagePullPolicy": "Always",
                            "command": ["gantry-runner"],
                            "env": [
                                {"name": "GANTRY_PLAN", "value": self.plan_json},
                                {"name": "GANTRY_NAMESPACES", "value": self.allowed_namespaces.join(",")},
                                {"name": "GANTRY_VALUES_DIR", "value": "/values"},
                                {"name": "GANTRY_PACKAGES_DIR", "value": "/work/packages"},
                                {"name": "RIVETER_AGE_KEY_FILE", "value": "/keys/age.key"},
                            ],
                            "envFrom": [{"secretRef": {"name": self.credentials_secret}}],
                            "volumeMounts": mounts,
                        }],
                        "volumes": volumes,
                    },
                },
            },
        })
    }
}

#[async_trait]
pub trait Cluster: Send + Sync {
    /// Creating a Job that already exists is success: a restarted service re-issues the same one.
    async fn create_job(&self, spec: &JobSpec) -> Result<(), ClusterError>;
    async fn job_status(&self, namespace: &str, name: &str) -> Result<JobStatus, ClusterError>;
    async fn job_log(&self, namespace: &str, name: &str) -> Result<String, ClusterError>;
    async fn delete_job(&self, namespace: &str, name: &str) -> Result<(), ClusterError>;
    /// Every workload a package installed, in `namespaces` (empty: everywhere Gantry can see).
    async fn list_workloads(&self, namespaces: &[String]) -> Result<Vec<Workload>, ClusterError>;
    /// Every resource of every kind that carries a package's label (empty `namespaces`: everywhere).
    async fn list_resources(
        &self,
        namespaces: &[String],
    ) -> Result<Vec<LiveResource>, ClusterError>;
    /// One resource as YAML, with what the API server adds (status, managed fields, ...) removed so it
    /// can be edited and applied back. A Secret is refused: its value is never shown.
    async fn resource_yaml(
        &self,
        api_version: Option<&str>,
        kind: &str,
        namespace: Option<&str>,
        name: &str,
    ) -> Result<String, ClusterError>;
}

/// The annotation Gantry puts on what it edited, so the difference from the package is visible.
pub const EDITED_ANNOTATION: &str = "gantry.forge/edited";

/// The label Riveter stamps on what a package installs.
pub const PACKAGE_LABEL: &str = "riveter.forge/package";
/// The annotation carrying the version (a label cannot hold `+`).
pub const VERSION_ANNOTATION: &str = "riveter.forge/package-version";

/// The cluster to talk to, and - only when it is the in-memory fake - a handle on it, which the
/// simulating runner needs to change it.
pub struct Connected {
    pub cluster: std::sync::Arc<dyn Cluster>,
    pub fake: Option<std::sync::Arc<FakeCluster>>,
}

pub async fn connect(settings: &Settings) -> Result<Connected, ClusterError> {
    Ok(match settings.cluster {
        ClusterMode::None => {
            let fake = std::sync::Arc::new(FakeCluster::new());
            if let Some(seed) = &settings.fake_cluster_seed {
                fake.load_seed(seed)?;
            }
            Connected {
                cluster: fake.clone(),
                fake: Some(fake),
            }
        }
        ClusterMode::InCluster | ClusterMode::Kubeconfig => Connected {
            cluster: std::sync::Arc::new(KubeCluster::connect(settings).await?),
            fake: None,
        },
    })
}

// ---------------------------------------------------------------- kube

pub struct KubeCluster {
    client: kube::Client,
    discovery: tokio::sync::Mutex<Option<(std::time::Instant, std::sync::Arc<kube::Discovery>)>>,
}

impl KubeCluster {
    pub async fn connect(settings: &Settings) -> Result<Self, ClusterError> {
        // kube builds its TLS stack on rustls, which wants a crypto provider chosen once per process.
        // The database layer happens to install one first in the service; this must not depend on that.
        let _ = rustls::crypto::ring::default_provider().install_default();

        let config = match (settings.cluster, &settings.kube_context) {
            (ClusterMode::Kubeconfig, Some(context)) => {
                kube::Config::from_kubeconfig(&kube::config::KubeConfigOptions {
                    context: Some(context.clone()),
                    ..Default::default()
                })
                .await
                .map_err(|e| ClusterError::Kube(e.to_string()))?
            }
            _ => kube::Config::infer()
                .await
                .map_err(|e| ClusterError::Kube(e.to_string()))?,
        };
        let client =
            kube::Client::try_from(config).map_err(|e| ClusterError::Kube(e.to_string()))?;
        Ok(Self {
            client,
            discovery: tokio::sync::Mutex::new(None),
        })
    }

    fn jobs(&self, namespace: &str) -> kube::Api<k8s_openapi::api::batch::v1::Job> {
        kube::Api::namespaced(self.client.clone(), namespace)
    }

    /// What kinds the cluster has. Asking costs a round of requests, so it is kept for a minute.
    async fn discovery(&self) -> Result<std::sync::Arc<kube::Discovery>, ClusterError> {
        let mut cached = self.discovery.lock().await;
        if let Some((at, found)) = cached.as_ref()
            && at.elapsed() < Duration::from_secs(60)
        {
            return Ok(found.clone());
        }
        let found = std::sync::Arc::new(
            kube::Discovery::new(self.client.clone())
                .run()
                .await
                .map_err(|e| ClusterError::Kube(e.to_string()))?,
        );
        *cached = Some((std::time::Instant::now(), found.clone()));
        Ok(found)
    }
}

#[async_trait]
impl Cluster for KubeCluster {
    async fn create_job(&self, spec: &JobSpec) -> Result<(), ClusterError> {
        let job: k8s_openapi::api::batch::v1::Job = serde_json::from_value(spec.manifest())
            .map_err(|e| ClusterError::Kube(format!("the Job manifest is invalid: {e}")))?;
        match self
            .jobs(&spec.namespace)
            .create(&kube::api::PostParams::default(), &job)
            .await
        {
            Ok(_) => Ok(()),
            Err(kube::Error::Api(status)) if status.code == 409 => Ok(()),
            Err(error) => Err(ClusterError::Kube(error.to_string())),
        }
    }

    async fn job_status(&self, namespace: &str, name: &str) -> Result<JobStatus, ClusterError> {
        let job = match self.jobs(namespace).get(name).await {
            Ok(job) => job,
            Err(kube::Error::Api(status)) if status.code == 404 => return Ok(JobStatus::Absent),
            Err(error) => return Err(ClusterError::Kube(error.to_string())),
        };
        let Some(status) = job.status else {
            return Ok(JobStatus::Active);
        };
        if status.succeeded.unwrap_or(0) > 0 {
            return Ok(JobStatus::Succeeded);
        }
        let failed = status
            .conditions
            .unwrap_or_default()
            .into_iter()
            .find(|condition| condition.type_ == "Failed" && condition.status == "True");
        Ok(match failed {
            Some(condition) => JobStatus::Failed(
                condition
                    .message
                    .or(condition.reason)
                    .unwrap_or_else(|| "the Job failed".to_string()),
            ),
            None => JobStatus::Active,
        })
    }

    async fn job_log(&self, namespace: &str, name: &str) -> Result<String, ClusterError> {
        let pods: kube::Api<k8s_openapi::api::core::v1::Pod> =
            kube::Api::namespaced(self.client.clone(), namespace);
        let list = pods
            .list(&kube::api::ListParams::default().labels(&format!("job-name={name}")))
            .await
            .map_err(|e| ClusterError::Kube(e.to_string()))?;
        let Some(pod) = list
            .items
            .into_iter()
            .max_by_key(|pod| pod.metadata.creation_timestamp.clone())
        else {
            return Ok(String::new());
        };
        let pod_name = pod.metadata.name.unwrap_or_default();
        pods.logs(&pod_name, &kube::api::LogParams::default())
            .await
            .map_err(|e| ClusterError::Kube(e.to_string()))
    }

    async fn delete_job(&self, namespace: &str, name: &str) -> Result<(), ClusterError> {
        let params = kube::api::DeleteParams::background();
        match self.jobs(namespace).delete(name, &params).await {
            Ok(_) => Ok(()),
            Err(kube::Error::Api(status)) if status.code == 404 => Ok(()),
            Err(error) => Err(ClusterError::Kube(error.to_string())),
        }
    }

    async fn list_workloads(&self, namespaces: &[String]) -> Result<Vec<Workload>, ClusterError> {
        use k8s_openapi::api::apps::v1::{DaemonSet, Deployment, StatefulSet};

        let params = kube::api::ListParams::default().labels(PACKAGE_LABEL);
        let scopes: Vec<Option<&str>> = if namespaces.is_empty() {
            vec![None]
        } else {
            namespaces.iter().map(|n| Some(n.as_str())).collect()
        };
        let kube_error = |e: kube::Error| ClusterError::Kube(e.to_string());

        let mut found = Vec::new();
        for scope in scopes {
            let api = |client: kube::Client| -> (
                kube::Api<Deployment>,
                kube::Api<StatefulSet>,
                kube::Api<DaemonSet>,
            ) {
                match scope {
                    Some(ns) => (
                        kube::Api::namespaced(client.clone(), ns),
                        kube::Api::namespaced(client.clone(), ns),
                        kube::Api::namespaced(client, ns),
                    ),
                    None => (
                        kube::Api::all(client.clone()),
                        kube::Api::all(client.clone()),
                        kube::Api::all(client),
                    ),
                }
            };
            let (deployments, statefulsets, daemonsets) = api(self.client.clone());

            for item in deployments.list(&params).await.map_err(kube_error)? {
                let ready = item
                    .status
                    .as_ref()
                    .and_then(|s| s.ready_replicas)
                    .unwrap_or(0);
                let desired = item.spec.as_ref().and_then(|s| s.replicas).unwrap_or(1);
                found.extend(workload("deployment", &item.metadata, desired, ready));
            }
            for item in statefulsets.list(&params).await.map_err(kube_error)? {
                let ready = item
                    .status
                    .as_ref()
                    .and_then(|s| s.ready_replicas)
                    .unwrap_or(0);
                let desired = item.spec.as_ref().and_then(|s| s.replicas).unwrap_or(1);
                found.extend(workload("statefulset", &item.metadata, desired, ready));
            }
            for item in daemonsets.list(&params).await.map_err(kube_error)? {
                let ready = item.status.as_ref().map_or(0, |s| s.number_ready);
                let desired = item
                    .status
                    .as_ref()
                    .map_or(0, |s| s.desired_number_scheduled);
                found.extend(workload("daemonset", &item.metadata, desired, ready));
            }
        }
        found.sort_by(|a, b| {
            (&a.namespace, &a.kind, &a.name).cmp(&(&b.namespace, &b.kind, &b.name))
        });
        Ok(found)
    }
    async fn list_resources(
        &self,
        namespaces: &[String],
    ) -> Result<Vec<LiveResource>, ClusterError> {
        use futures_util::StreamExt;

        let discovery = self.discovery().await?;
        let params = kube::api::ListParams::default().labels(PACKAGE_LABEL);

        let mut kinds = Vec::new();
        for group in discovery.groups() {
            for (resource, capabilities) in group.recommended_resources() {
                if !capabilities.supports_operation(kube::discovery::verbs::LIST)
                    || SKIPPED_KINDS.contains(&resource.kind.as_str())
                    || resource.kind == "Secret"
                {
                    continue;
                }
                kinds.push((resource, capabilities));
            }
        }

        // One list per kind, a few at a time. A kind this account may not list answers 403 and is simply
        // not shown: what the service is allowed to see is a decision made by its Role, not here.
        let client = self.client.clone();
        let results: Vec<Vec<LiveResource>> = futures_util::stream::iter(kinds)
            .map(|(resource, capabilities)| {
                let client = client.clone();
                let params = params.clone();
                async move {
                    // `all` covers a cluster-scoped kind and every namespace of a namespaced one.
                    let _ = capabilities;
                    let api: kube::Api<kube::api::DynamicObject> =
                        kube::Api::all_with(client, &resource);
                    match api.list(&params).await {
                        Ok(list) => list
                            .items
                            .into_iter()
                            .filter_map(|object| live_resource(&resource, &object))
                            .collect(),
                        Err(_) => Vec::new(),
                    }
                }
            })
            .buffer_unordered(8)
            .collect()
            .await;

        let mut found: Vec<LiveResource> = results
            .into_iter()
            .flatten()
            // The allow-list names namespaces; what has no namespace is cluster-scoped, and is shown so a
            // package's roles are not hidden, though acting on it is refused outside an empty allow-list.
            .filter(|r| {
                namespaces.is_empty()
                    || r.namespace.is_none()
                    || r.namespace
                        .as_ref()
                        .is_some_and(|ns| namespaces.contains(ns))
            })
            .collect();
        found.sort_by(|a, b| (&a.package, &a.kind, &a.name).cmp(&(&b.package, &b.kind, &b.name)));
        Ok(found)
    }

    async fn resource_yaml(
        &self,
        api_version: Option<&str>,
        kind: &str,
        namespace: Option<&str>,
        name: &str,
    ) -> Result<String, ClusterError> {
        if kind.eq_ignore_ascii_case("secret") {
            return Err(ClusterError::Kube(
                "a Secret is never shown; set its values through the package's values Secret"
                    .into(),
            ));
        }
        let discovery = self.discovery().await?;
        let (resource, capabilities) = discovery
            .groups()
            .flat_map(kube::discovery::ApiGroup::recommended_resources)
            .find(|(resource, _)| {
                resource.kind.eq_ignore_ascii_case(kind)
                    && api_version.is_none_or(|v| v == resource.api_version)
            })
            .ok_or_else(|| ClusterError::Kube(format!("the cluster has no kind {kind}")))?;

        let api: kube::Api<kube::api::DynamicObject> = match (capabilities.scope, namespace) {
            (kube::discovery::Scope::Namespaced, Some(ns)) => {
                kube::Api::namespaced_with(self.client.clone(), ns, &resource)
            }
            _ => kube::Api::all_with(self.client.clone(), &resource),
        };
        let object = api
            .get(name)
            .await
            .map_err(|e| ClusterError::Kube(e.to_string()))?;
        let mut value =
            serde_json::to_value(&object).map_err(|e| ClusterError::Kube(e.to_string()))?;
        clean_for_editing(&mut value);
        serde_yaml::to_string(&value).map_err(|e| ClusterError::Kube(e.to_string()))
    }
}

/// Kinds that are never a package's own: derived objects and noise.
const SKIPPED_KINDS: [&str; 9] = [
    "Event",
    "Endpoints",
    "EndpointSlice",
    "Pod",
    "ReplicaSet",
    "ControllerRevision",
    "Lease",
    "Node",
    "ComponentStatus",
];

fn live_resource(
    resource: &kube::api::ApiResource,
    object: &kube::api::DynamicObject,
) -> Option<LiveResource> {
    let meta = &object.metadata;
    let package = meta.labels.as_ref()?.get(PACKAGE_LABEL)?.clone();
    let annotations = meta.annotations.as_ref();
    let spec_replicas = object
        .data
        .pointer("/spec/replicas")
        .and_then(serde_json::Value::as_i64);
    let ready_replicas = object
        .data
        .pointer("/status/readyReplicas")
        .and_then(serde_json::Value::as_i64);
    let ready = match resource.kind.as_str() {
        "Deployment" | "StatefulSet" => Some(format!(
            "{}/{}",
            ready_replicas.unwrap_or(0),
            spec_replicas.unwrap_or(1)
        )),
        "DaemonSet" => Some(format!(
            "{}/{}",
            object
                .data
                .pointer("/status/numberReady")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(0),
            object
                .data
                .pointer("/status/desiredNumberScheduled")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(0)
        )),
        "Job" => Some(
            if object
                .data
                .pointer("/status/succeeded")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(0)
                > 0
            {
                "done".to_string()
            } else if object
                .data
                .pointer("/status/failed")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(0)
                > 0
            {
                "failed".to_string()
            } else {
                "running".to_string()
            },
        ),
        _ => None,
    };
    Some(LiveResource {
        api_version: Some(resource.api_version.clone()),
        kind: resource.kind.clone(),
        name: meta.name.clone()?,
        namespace: meta.namespace.clone(),
        package,
        version: annotations.and_then(|a| a.get(VERSION_ANNOTATION)).cloned(),
        ready,
        edited: annotations.is_some_and(|a| a.contains_key(EDITED_ANNOTATION)),
        owned: meta
            .owner_references
            .as_ref()
            .is_some_and(|owners| !owners.is_empty()),
    })
}

/// Removes what the API server added, so the YAML is what a person edits and applies back.
fn clean_for_editing(value: &mut serde_json::Value) {
    if let Some(object) = value.as_object_mut() {
        object.remove("status");
        if let Some(meta) = object.get_mut("metadata").and_then(|m| m.as_object_mut()) {
            for field in [
                "managedFields",
                "resourceVersion",
                "uid",
                "creationTimestamp",
                "generation",
                "selfLink",
                "ownerReferences",
            ] {
                meta.remove(field);
            }
            if let Some(annotations) = meta.get_mut("annotations").and_then(|a| a.as_object_mut()) {
                annotations.remove("kubectl.kubernetes.io/last-applied-configuration");
                annotations.remove("deployment.kubernetes.io/revision");
            }
        }
    }
}

fn workload(
    kind: &str,
    meta: &k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta,
    desired: i32,
    ready: i32,
) -> Option<Workload> {
    Some(Workload {
        kind: kind.to_string(),
        name: meta.name.clone()?,
        namespace: meta.namespace.clone()?,
        package: meta.labels.as_ref()?.get(PACKAGE_LABEL)?.clone(),
        version: meta
            .annotations
            .as_ref()
            .and_then(|a| a.get(VERSION_ANNOTATION))
            .cloned(),
        desired,
        ready,
    })
}

// ---------------------------------------------------------------- fake

/// What a fake Job does, set by a test to script the cluster.
#[derive(Clone, Debug)]
pub struct FakeJob {
    pub spec: JobSpec,
    pub status: JobStatus,
    pub log: String,
}

/// A non-workload resource in the fake cluster, with the YAML a person would edit.
#[derive(Clone, Debug)]
pub struct Extra {
    pub live: LiveResource,
    pub yaml: String,
}

#[derive(Default)]
pub struct FakeCluster {
    jobs: Mutex<BTreeMap<String, FakeJob>>,
    workloads: Mutex<Vec<Workload>>,
    extras: Mutex<Vec<Extra>>,
    /// What each package declares, for the simulating runner to put back.
    catalog: Mutex<BTreeMap<String, Vec<crate::domain::resources::InventoryItem>>>,
    /// `(kind, name, namespace)` of what was edited, lower-cased kind.
    edited: Mutex<std::collections::BTreeSet<(String, String, String)>>,
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn workload_live(workload: &Workload, edited: bool) -> LiveResource {
    LiveResource {
        api_version: Some("apps/v1".to_string()),
        kind: match workload.kind.as_str() {
            "statefulset" => "StatefulSet".to_string(),
            "daemonset" => "DaemonSet".to_string(),
            _ => "Deployment".to_string(),
        },
        name: workload.name.clone(),
        namespace: Some(workload.namespace.clone()),
        package: workload.package.clone(),
        version: workload.version.clone(),
        ready: Some(format!("{}/{}", workload.ready, workload.desired)),
        edited,
        owned: false,
    }
}

fn workload_yaml(workload: &Workload) -> String {
    format!(
        "apiVersion: apps/v1\nkind: {}\nmetadata:\n  name: {}\n  namespace: {}\nspec:\n  replicas: {}\n  template:\n    spec:\n      containers:\n      - name: {}\n        image: example.invalid/{}:1\n",
        match workload.kind.as_str() {
            "statefulset" => "StatefulSet",
            "daemonset" => "DaemonSet",
            _ => "Deployment",
        },
        workload.name,
        workload.namespace,
        workload.desired,
        workload.name,
        workload.name
    )
}

impl FakeCluster {
    pub fn new() -> Self {
        Self::default()
    }

    fn jobs(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, FakeJob>> {
        self.jobs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// A test finishing (or failing) a Job the service created.
    pub fn complete(&self, name: &str, status: JobStatus, log: &str) {
        if let Some(job) = self.jobs().get_mut(name) {
            job.status = status;
            job.log = log.to_string();
        }
    }

    pub fn job(&self, name: &str) -> Option<FakeJob> {
        self.jobs().get(name).cloned()
    }

    pub fn job_names(&self) -> Vec<String> {
        self.jobs().keys().cloned().collect()
    }

    /// What the fake cluster runs, for the planner and the targets view to find.
    pub fn set_workloads(&self, workloads: Vec<Workload>) {
        *self
            .workloads
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = workloads;
    }

    /// Starts the fake cluster from a JSON file: `{"workloads": [...], "resources": [...]}` (an array is
    /// taken as just the workloads). Every resource also becomes part of its package's catalog, which is
    /// what the simulating runner puts back when something is applied again.
    pub fn load_seed(&self, path: &std::path::Path) -> Result<(), ClusterError> {
        let bad =
            |e: &dyn std::fmt::Display| ClusterError::Kube(format!("{}: {e}", path.display()));
        let text = std::fs::read_to_string(path).map_err(|e| bad(&e))?;
        let value: serde_json::Value = serde_json::from_str(&text).map_err(|e| bad(&e))?;
        let (workloads, resources, declared) = match value {
            serde_json::Value::Array(_) => (
                value,
                serde_json::Value::Array(Vec::new()),
                serde_json::Value::Array(Vec::new()),
            ),
            serde_json::Value::Object(mut object) => (
                object
                    .remove("workloads")
                    .unwrap_or_else(|| serde_json::json!([])),
                object
                    .remove("resources")
                    .unwrap_or_else(|| serde_json::json!([])),
                object
                    .remove("declared")
                    .unwrap_or_else(|| serde_json::json!([])),
            ),
            _ => return Err(bad(&"expected an object or an array")),
        };
        // Declared by a package but not in the cluster: stopped, or never applied.
        let declared: Vec<LiveResource> = serde_json::from_value(declared).map_err(|e| bad(&e))?;
        for resource in &declared {
            self.catalog_add(&resource.package, resource);
        }
        let workloads: Vec<Workload> = serde_json::from_value(workloads).map_err(|e| bad(&e))?;
        let resources: Vec<LiveResource> =
            serde_json::from_value(resources).map_err(|e| bad(&e))?;

        for workload in &workloads {
            self.catalog_add(&workload.package, &workload_live(workload, false));
        }
        for resource in resources {
            self.catalog_add(&resource.package, &resource);
            let yaml = format!(
                "apiVersion: {}\nkind: {}\nmetadata:\n  name: {}\n{}",
                resource
                    .api_version
                    .clone()
                    .unwrap_or_else(|| "v1".to_string()),
                resource.kind,
                resource.name,
                resource
                    .namespace
                    .as_ref()
                    .map_or(String::new(), |ns| format!("  namespace: {ns}\n"))
            );
            self.upsert_extra(Extra {
                live: resource,
                yaml,
            });
        }
        self.set_workloads(workloads);
        Ok(())
    }

    fn catalog_add(&self, package: &str, live: &LiveResource) {
        let item = crate::domain::resources::InventoryItem {
            api_version: live.api_version.clone(),
            kind: live.kind.clone(),
            name: live.name.clone(),
            namespace: live.namespace.clone(),
        };
        let mut catalog = lock(&self.catalog);
        let items = catalog.entry(package.to_string()).or_default();
        if !items.contains(&item) {
            items.push(item);
        }
    }

    /// Sets what a package declares, for the simulation to put back.
    pub fn set_catalog(&self, package: &str, items: Vec<crate::domain::resources::InventoryItem>) {
        lock(&self.catalog).insert(package.to_string(), items);
    }

    /// Every package the fake cluster has something of, with the version its workloads run.
    pub fn seeded_packages(&self) -> Vec<(String, Option<String>)> {
        let mut seen: Vec<(String, Option<String>)> = Vec::new();
        for resource in lock(&self.workloads)
            .iter()
            .map(|w| (w.package.clone(), w.version.clone()))
        {
            if !seen.iter().any(|(p, _)| *p == resource.0) {
                seen.push(resource);
            }
        }
        seen
    }

    /// What a package declares, as the simulation knows it.
    pub fn catalog(&self, package: &str) -> Vec<crate::domain::resources::InventoryItem> {
        lock(&self.catalog)
            .get(package)
            .cloned()
            .unwrap_or_default()
    }

    pub fn upsert_extra(&self, extra: Extra) {
        let mut extras = lock(&self.extras);
        match extras.iter_mut().find(|e| {
            e.live.kind == extra.live.kind
                && e.live.name == extra.live.name
                && e.live.namespace == extra.live.namespace
        }) {
            Some(existing) => *existing = extra,
            None => extras.push(extra),
        }
    }

    /// Removes a resource of any kind. `true` if it was there.
    pub fn delete_resource(&self, kind: &str, name: &str, namespace: Option<&str>) -> bool {
        let before = self.workloads().len() + lock(&self.extras).len();
        let mut workloads = lock(&self.workloads);
        workloads.retain(|w| {
            !(w.kind.eq_ignore_ascii_case(kind)
                && w.name == name
                && Some(w.namespace.as_str()) == namespace)
        });
        let after_workloads = workloads.len();
        drop(workloads);
        lock(&self.extras).retain(|e| {
            !(e.live.kind.eq_ignore_ascii_case(kind)
                && e.live.name == name
                && e.live.namespace.as_deref() == namespace)
        });
        let after = after_workloads + lock(&self.extras).len();
        after < before
    }

    /// Applies edited YAML: a workload's replicas follow it, anything else just keeps the text.
    pub fn apply_yaml(&self, yaml: &str) -> Result<(), String> {
        let doc: serde_yaml::Value = serde_yaml::from_str(yaml).map_err(|e| e.to_string())?;
        let text = |v: Option<&serde_yaml::Value>| {
            v.and_then(serde_yaml::Value::as_str).map(str::to_string)
        };
        let kind = text(doc.get("kind")).ok_or("no kind")?;
        let name =
            text(doc.get("metadata").and_then(|m| m.get("name"))).ok_or("no metadata.name")?;
        let namespace = text(doc.get("metadata").and_then(|m| m.get("namespace")));
        lock(&self.edited).insert((
            kind.to_ascii_lowercase(),
            name.clone(),
            namespace.clone().unwrap_or_default(),
        ));

        let mut workloads = lock(&self.workloads);
        if let Some(w) = workloads.iter_mut().find(|w| {
            w.kind.eq_ignore_ascii_case(&kind)
                && w.name == name
                && Some(w.namespace.clone()) == namespace
        }) {
            if let Some(replicas) = doc
                .get("spec")
                .and_then(|s| s.get("replicas"))
                .and_then(serde_yaml::Value::as_i64)
            {
                w.desired = i32::try_from(replicas).unwrap_or(i32::MAX);
                w.ready = w.desired;
            }
            return Ok(());
        }
        drop(workloads);
        let mut extras = lock(&self.extras);
        if let Some(extra) = extras.iter_mut().find(|e| {
            e.live.kind.eq_ignore_ascii_case(&kind)
                && e.live.name == name
                && e.live.namespace == namespace
        }) {
            extra.yaml = yaml.to_string();
            extra.live.edited = true;
            return Ok(());
        }
        Err(format!("{kind}/{name} does not exist"))
    }

    pub fn workloads(&self) -> Vec<Workload> {
        self.workloads
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Adds a workload, or replaces the one with the same namespace, kind and name.
    pub fn upsert_workload(&self, workload: Workload) {
        let mut all = self
            .workloads
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match all.iter_mut().find(|w| {
            w.namespace == workload.namespace && w.kind == workload.kind && w.name == workload.name
        }) {
            Some(existing) => *existing = workload,
            None => all.push(workload),
        }
    }

    /// A restart-and-lose-everything: the Job vanishes with no trace.
    pub fn forget(&self, name: &str) {
        self.jobs().remove(name);
    }
}

#[async_trait]
impl Cluster for FakeCluster {
    async fn create_job(&self, spec: &JobSpec) -> Result<(), ClusterError> {
        self.jobs()
            .entry(spec.name.clone())
            .or_insert_with(|| FakeJob {
                spec: spec.clone(),
                status: JobStatus::Active,
                log: String::new(),
            });
        Ok(())
    }

    async fn job_status(&self, _namespace: &str, name: &str) -> Result<JobStatus, ClusterError> {
        Ok(self
            .jobs()
            .get(name)
            .map_or(JobStatus::Absent, |job| job.status.clone()))
    }

    async fn job_log(&self, _namespace: &str, name: &str) -> Result<String, ClusterError> {
        Ok(self
            .jobs()
            .get(name)
            .map(|job| job.log.clone())
            .unwrap_or_default())
    }

    async fn delete_job(&self, _namespace: &str, name: &str) -> Result<(), ClusterError> {
        self.jobs().remove(name);
        Ok(())
    }

    async fn list_resources(
        &self,
        namespaces: &[String],
    ) -> Result<Vec<LiveResource>, ClusterError> {
        let edited = lock(&self.edited).clone();
        let mut all: Vec<LiveResource> = lock(&self.workloads)
            .iter()
            .map(|w| {
                let key = (
                    w.kind.to_ascii_lowercase(),
                    w.name.clone(),
                    w.namespace.clone(),
                );
                workload_live(w, edited.contains(&key))
            })
            .collect();
        all.extend(lock(&self.extras).iter().map(|e| e.live.clone()));
        all.retain(|r| {
            namespaces.is_empty()
                || r.namespace
                    .as_ref()
                    .is_none_or(|ns| namespaces.contains(ns))
        });
        Ok(all)
    }

    async fn resource_yaml(
        &self,
        _api_version: Option<&str>,
        kind: &str,
        namespace: Option<&str>,
        name: &str,
    ) -> Result<String, ClusterError> {
        if kind.eq_ignore_ascii_case("secret") {
            return Err(ClusterError::Kube("a Secret is never shown".into()));
        }
        if let Some(w) = lock(&self.workloads).iter().find(|w| {
            w.kind.eq_ignore_ascii_case(kind)
                && w.name == name
                && Some(w.namespace.as_str()) == namespace
        }) {
            return Ok(workload_yaml(w));
        }
        lock(&self.extras)
            .iter()
            .find(|e| {
                e.live.kind.eq_ignore_ascii_case(kind)
                    && e.live.name == name
                    && e.live.namespace.as_deref() == namespace
            })
            .map(|e| e.yaml.clone())
            .ok_or_else(|| ClusterError::Kube(format!("{kind}/{name} not found")))
    }

    async fn list_workloads(&self, namespaces: &[String]) -> Result<Vec<Workload>, ClusterError> {
        Ok(self
            .workloads
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .filter(|w| namespaces.is_empty() || namespaces.contains(&w.namespace))
            .cloned()
            .collect())
    }
}
