//! Configuration, and the one rule it enforces: the safe default for anything that changes a cluster is to
//! not change it. Run with nothing set, Gantry has no cluster at all and plans in dry-run; touching a real
//! one has to be said out loud, twice (`GANTRY_CLUSTER` and `GANTRY_RUNNER`).

use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClusterMode {
    /// The service account of the pod it runs in.
    InCluster,
    /// The developer's kubeconfig (and `GANTRY_KUBE_CONTEXT` to pin a context).
    Kubeconfig,
    /// An in-memory fake: no cluster is contacted.
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunnerMode {
    /// A Kubernetes Job in the cluster. The only mode that survives the service restarting itself.
    Job,
    /// A child process of the service, with the developer's own `kubectl` and `riveter`.
    Local,
    /// A child process that prints what it would change and changes nothing.
    DryRun,
    /// Applies each step to the in-memory fake cluster, in-process, so a person can click through
    /// start, stop, swap and upgrade with no cluster and no runner and watch the state change. Only with
    /// `GANTRY_CLUSTER=none`: it simulates, it does not run `riveter` or `kubectl`.
    Simulate,
}

#[derive(Clone, Debug)]
pub struct Settings {
    pub cluster: ClusterMode,
    pub runner: RunnerMode,
    pub kube_context: Option<String>,
    /// The namespace Gantry and its runner Jobs live in.
    pub namespace: String,
    pub runner_image: String,
    pub runner_service_account: String,
    /// The Secret that carries the registry credentials into a runner Job.
    pub credentials_secret: String,
    /// The Secret holding the age key (`age.key`) that opens a package's encrypted values, mounted into
    /// every runner Job. Optional: a package with no encrypted values needs none.
    pub age_key_secret: String,
    /// Namespaces Gantry may plan for or touch; empty means any.
    pub allowed_namespaces: Vec<String>,
    /// Where local runners keep their logs and exit codes.
    pub state_dir: PathBuf,
    /// The `gantry-runner` executable a local runner starts.
    pub runner_bin: String,
    /// Local runs read `<values_dir>/<secret>/env`.
    pub values_dir: PathBuf,
    /// A Job that runs longer than this is stopped.
    pub job_deadline_secs: u64,
    /// Warehouse, including its base path (`https://host/warehouse`).
    pub warehouse_url: Option<String>,
    /// Gatehouse, where Gantry's machine token for Warehouse comes from.
    pub gatehouse_url: Option<String>,
    pub warehouse_client_id: String,
    pub warehouse_client_secret: Option<String>,
    pub tls_verify: bool,
    /// Packages as `.rivet` files in a directory, instead of Warehouse: running with no registry at all.
    pub packages_dir: Option<PathBuf>,
    /// Gantry's own Deployment, as `namespace/name`: the one unit that must go last, with a rollback.
    pub self_deployment: Option<(String, String)>,
    /// Workloads (`namespace/name`) that no stop may take down: Gantry itself, and what it stands on.
    /// Protected from being *stopped*, never from being upgraded.
    pub protected: Vec<(String, String)>,
    /// How long the guarded rollout of Gantry's own Deployment waits for the new pod before the runner
    /// rolls it back.
    pub self_rollout_timeout_secs: u64,
    /// A JSON file of workloads the fake cluster starts with (`GANTRY_FAKE_CLUSTER`).
    pub fake_cluster_seed: Option<PathBuf>,
}

impl Settings {
    pub fn from_env() -> Result<Self, String> {
        Self::from_lookup(|key| std::env::var(key).ok().filter(|value| !value.is_empty()))
    }

    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Result<Self, String> {
        let in_pod = get("KUBERNETES_SERVICE_HOST").is_some();

        let cluster = match get("GANTRY_CLUSTER").as_deref() {
            Some("in-cluster") => ClusterMode::InCluster,
            Some("kubeconfig") => ClusterMode::Kubeconfig,
            Some("none") => ClusterMode::None,
            Some(other) => {
                return Err(format!(
                    "GANTRY_CLUSTER '{other}' is not in-cluster, kubeconfig or none"
                ));
            }
            None if in_pod => ClusterMode::InCluster,
            None => ClusterMode::None,
        };

        let runner = match get("GANTRY_RUNNER").as_deref() {
            Some("job") => RunnerMode::Job,
            Some("local") => RunnerMode::Local,
            Some("dry-run") => RunnerMode::DryRun,
            Some("simulate") => RunnerMode::Simulate,
            Some(other) => {
                return Err(format!(
                    "GANTRY_RUNNER '{other}' is not job, local, dry-run or simulate"
                ));
            }
            // Only a pod defaults to changing things; a developer's machine has to ask.
            None if cluster == ClusterMode::InCluster => RunnerMode::Job,
            None => RunnerMode::DryRun,
        };

        match (runner, cluster) {
            (RunnerMode::Job, ClusterMode::None) => {
                return Err("GANTRY_RUNNER=job needs a cluster to put the Job in; \
                            set GANTRY_CLUSTER=in-cluster or kubeconfig"
                    .to_string());
            }
            (RunnerMode::Simulate, ClusterMode::Kubeconfig | ClusterMode::InCluster) => {
                return Err("GANTRY_RUNNER=simulate changes the in-memory fake cluster only, so it \
                            needs GANTRY_CLUSTER=none - against a real cluster it would show changes \
                            that never happened"
                    .to_string());
            }
            (RunnerMode::Local, ClusterMode::None | ClusterMode::InCluster) => {
                return Err(
                    "GANTRY_RUNNER=local runs kubectl as the developer, so it needs \
                            GANTRY_CLUSTER=kubeconfig - never the fake cluster, which would \
                            report one thing while kubectl changed another"
                        .to_string(),
                );
            }
            _ => {}
        }

        let namespace = get("GANTRY_NAMESPACE")
            .or_else(|| {
                std::fs::read_to_string("/var/run/secrets/kubernetes.io/serviceaccount/namespace")
                    .ok()
                    .map(|namespace| namespace.trim().to_string())
            })
            .unwrap_or_else(|| "forge".to_string());

        Ok(Self {
            cluster,
            runner,
            kube_context: get("GANTRY_KUBE_CONTEXT"),
            namespace,
            runner_image: get("GANTRY_RUNNER_IMAGE")
                .unwrap_or_else(|| "forge/gantry/runner:latest".to_string()),
            runner_service_account: get("GANTRY_RUNNER_SERVICE_ACCOUNT")
                .unwrap_or_else(|| "gantry-runner".to_string()),
            credentials_secret: get("GANTRY_RUNNER_CREDENTIALS_SECRET")
                .unwrap_or_else(|| "gantry-runner-credentials".to_string()),
            age_key_secret: get("GANTRY_AGE_KEY_SECRET")
                .unwrap_or_else(|| "gantry-age-key".to_string()),
            allowed_namespaces: get("GANTRY_NAMESPACES")
                .map(|list| {
                    list.split(',')
                        .map(str::trim)
                        .filter(|namespace| !namespace.is_empty())
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
            state_dir: get("GANTRY_STATE_DIR")
                .map_or_else(|| std::env::temp_dir().join("gantry"), PathBuf::from),
            runner_bin: get("GANTRY_RUNNER_BIN").unwrap_or_else(|| "gantry-runner".to_string()),
            values_dir: get("GANTRY_VALUES_DIR")
                .map_or_else(|| PathBuf::from("values"), PathBuf::from),
            job_deadline_secs: get("GANTRY_JOB_DEADLINE_SECS")
                .and_then(|seconds| seconds.parse().ok())
                .unwrap_or(3600),
            warehouse_url: get("WAREHOUSE_URL"),
            gatehouse_url: get("GATEHOUSE_URL"),
            warehouse_client_id: get("WAREHOUSE_CLIENT_ID")
                .unwrap_or_else(|| "gantry-warehouse".to_string()),
            warehouse_client_secret: get("WAREHOUSE_CLIENT_SECRET"),
            tls_verify: get("GATEHOUSE_TLS_VERIFY").is_none_or(|v| v != "false"),
            packages_dir: get("GANTRY_PACKAGES_DIR").map(PathBuf::from),
            self_deployment: parse_self(
                get("GANTRY_SELF_DEPLOYMENT")
                    .as_deref()
                    .unwrap_or("forge/gantry"),
            ),
            fake_cluster_seed: get("GANTRY_FAKE_CLUSTER").map(PathBuf::from),
            self_rollout_timeout_secs: get("GANTRY_SELF_ROLLOUT_TIMEOUT_SECS")
                .and_then(|seconds| seconds.parse().ok())
                .unwrap_or(300),
            protected: get("GANTRY_PROTECTED")
                .unwrap_or_else(|| {
                    "forge/gantry,forge/forge-db,forge/redis,forge/gatehouse".to_string()
                })
                .split(',')
                .filter_map(|entry| parse_self(entry.trim()))
                .collect(),
        })
    }

    /// A fully local, fully inert configuration: no cluster, dry-run. What tests and a bare run use.
    pub fn inert() -> Self {
        Self::from_lookup(|_| None).expect("the defaults are consistent")
    }
}

/// `namespace/name`.
fn parse_self(value: &str) -> Option<(String, String)> {
    let (namespace, name) = value.split_once('/')?;
    (!namespace.is_empty() && !name.is_empty()).then(|| (namespace.to_string(), name.to_string()))
}
