//! The cluster as it is, not as a package left it: every kind a person looks at (nodes, pods, services,
//! volumes, accounts, ...), listed and shown the same way. This is what Palantir used to be.
//!
//! Read-only, and never a Secret: its value is not something to look at, and Gantry's role cannot read one.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::option_if_let_else,
    reason = "quantities for reading: a rounded figure, never a count that matters"
)]

use quench_serde::{Serialize, Value};

/// The kinds shown, in the order and under the heading a person looks for them.
///
/// A kind the cluster does not serve (or this account may not list) is left out of what
/// [`crate::domain::cluster::Cluster::browse_kinds`] returns.
pub const KINDS: [(&str, &str); 25] = [
    ("Cluster", "Node"),
    ("Cluster", "Namespace"),
    ("Cluster", "Event"),
    ("Workloads", "Pod"),
    ("Workloads", "Deployment"),
    ("Workloads", "StatefulSet"),
    ("Workloads", "DaemonSet"),
    ("Workloads", "ReplicaSet"),
    ("Workloads", "Job"),
    ("Workloads", "CronJob"),
    ("Network", "Service"),
    ("Network", "Ingress"),
    ("Network", "NetworkPolicy"),
    ("Config", "ConfigMap"),
    ("Storage", "PersistentVolumeClaim"),
    ("Storage", "PersistentVolume"),
    ("Storage", "StorageClass"),
    ("Accounts", "ServiceAccount"),
    ("Accounts", "Role"),
    ("Accounts", "RoleBinding"),
    ("Accounts", "ClusterRole"),
    ("Accounts", "ClusterRoleBinding"),
    ("Custom", "Certificate"),
    ("Custom", "Middleware"),
    ("Custom", "IngressRoute"),
];

/// A kind that can be listed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct KindInfo {
    pub kind: String,
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub namespaced: bool,
    /// The heading it is listed under.
    pub group: String,
}

/// One object in a list: what a row of a table needs, whatever the kind.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Entry {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub kind: String,
    pub name: String,
    pub namespace: Option<String>,
    /// `Running`, `3/3`, `Ready`, `Bound`...: the one word a person scans for.
    pub status: String,
    /// What else is worth a column for this kind (the node a pod is on, a service's ports, ...).
    pub detail: String,
    /// When it was created, RFC 3339.
    pub created: Option<String>,
    /// Whether `status` is a problem (a crash loop, a node not ready), so it can be marked.
    pub attention: bool,
    /// What it uses right now (`134m · 1.2Gi`), for the kinds metrics-server reports; empty otherwise.
    pub usage: String,
}

/// An event about an object.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct EventInfo {
    /// `Normal` or `Warning`.
    #[serde(rename = "type")]
    pub kind: String,
    pub reason: String,
    pub message: String,
    pub count: i64,
    pub last: Option<String>,
    /// What it is about: `Pod/name`, for the events of a whole namespace.
    pub object: String,
}

/// One object, as YAML, with what happened to it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ObjectView {
    pub yaml: String,
    pub events: Vec<EventInfo>,
    /// The containers of a pod, for its logs.
    pub containers: Vec<String>,
    /// The YAML to edit: without what the API server adds (status, ids, managed fields). Empty for what is
    /// not edited from here.
    pub edit_yaml: String,
    /// What belongs to it: the pods a workload or a service selects, the pods on a node.
    pub related: Vec<Entry>,
}

/// The heading `kind` is listed under, if it is one of the shown ones.
#[must_use]
pub fn group_of(kind: &str) -> Option<&'static str> {
    KINDS
        .iter()
        .find(|(_, k)| k.eq_ignore_ascii_case(kind))
        .map(|(group, _)| *group)
}

fn text<'a>(object: &'a Value, pointer: &str) -> Option<&'a str> {
    object.pointer(pointer).and_then(Value::as_str)
}

fn number(object: &Value, pointer: &str) -> i64 {
    object
        .pointer(pointer)
        .and_then(Value::as_i64)
        .unwrap_or_default()
}

fn len(object: &Value, pointer: &str) -> usize {
    object.pointer(pointer).map_or(0, |value| {
        value
            .as_array()
            .map(Vec::len)
            .or_else(|| value.as_object().map(quench_serde::Map::len))
            .unwrap_or_default()
    })
}

/// What the status of a pod really is: the phase, unless a container says better (`CrashLoopBackOff`,
/// `ImagePullBackOff`, `Completed`).
fn pod_status(object: &Value) -> (String, bool) {
    if text(object, "/metadata/deletionTimestamp").is_some() {
        return ("Terminating".into(), false);
    }
    let phase = text(object, "/status/phase").unwrap_or("Unknown");
    let statuses = object
        .pointer("/status/containerStatuses")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for container in &statuses {
        if let Some(reason) = text(container, "/state/waiting/reason") {
            return (reason.to_string(), reason != "ContainerCreating");
        }
        if let Some(reason) = text(container, "/state/terminated/reason")
            && phase != "Succeeded"
            && reason != "Completed"
        {
            return (reason.to_string(), true);
        }
    }
    let bad = matches!(phase, "Failed" | "Unknown");
    (phase.to_string(), bad)
}

fn first_image(object: &Value) -> String {
    text(object, "/spec/template/spec/containers/0/image")
        .or_else(|| {
            text(
                object,
                "/spec/jobTemplate/spec/template/spec/containers/0/image",
            )
        })
        .unwrap_or_default()
        .to_string()
}

fn ports(object: &Value) -> String {
    object
        .pointer("/spec/ports")
        .and_then(Value::as_array)
        .map(|ports| {
            ports
                .iter()
                .map(|port| {
                    format!(
                        "{}/{}",
                        port.get("port").and_then(Value::as_i64).unwrap_or_default(),
                        port.get("protocol")
                            .and_then(Value::as_str)
                            .unwrap_or("TCP")
                    )
                })
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default()
}

/// The status word, whether it is a problem, and the detail column of `object` as a `kind`.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn summarize(kind: &str, object: &Value) -> (String, bool, String) {
    match kind {
        "Pod" => {
            let (status, bad) = pod_status(object);
            let statuses = object
                .pointer("/status/containerStatuses")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let ready = statuses
                .iter()
                .filter(|c| c.get("ready").and_then(Value::as_bool) == Some(true))
                .count();
            let restarts: i64 = statuses
                .iter()
                .map(|c| {
                    c.get("restartCount")
                        .and_then(Value::as_i64)
                        .unwrap_or_default()
                })
                .sum();
            let total = len(object, "/spec/containers");
            let node = text(object, "/spec/nodeName").unwrap_or("-");
            (
                status,
                bad,
                format!("{ready}/{total} · {restarts} restarts · {node}"),
            )
        }
        "Deployment" | "StatefulSet" | "ReplicaSet" => {
            let want = object
                .pointer("/spec/replicas")
                .and_then(Value::as_i64)
                .unwrap_or(1);
            let have = number(object, "/status/readyReplicas");
            (format!("{have}/{want}"), have < want, first_image(object))
        }
        "DaemonSet" => {
            let want = number(object, "/status/desiredNumberScheduled");
            let have = number(object, "/status/numberReady");
            (format!("{have}/{want}"), have < want, first_image(object))
        }
        "Job" => {
            let status = if number(object, "/status/succeeded") > 0 {
                "Complete"
            } else if number(object, "/status/failed") > 0 {
                "Failed"
            } else {
                "Running"
            };
            (status.into(), status == "Failed", first_image(object))
        }
        "CronJob" => (
            if object.pointer("/spec/suspend").and_then(Value::as_bool) == Some(true) {
                "Suspended"
            } else {
                "Active"
            }
            .into(),
            false,
            text(object, "/spec/schedule")
                .unwrap_or_default()
                .to_string(),
        ),
        "Node" => {
            let ready = object
                .pointer("/status/conditions")
                .and_then(Value::as_array)
                .and_then(|all| all.iter().find(|c| text(c, "/type") == Some("Ready")))
                .and_then(|c| text(c, "/status"))
                == Some("True");
            let roles: Vec<&str> = object
                .pointer("/metadata/labels")
                .and_then(Value::as_object)
                .map(|labels| {
                    labels
                        .keys()
                        .filter_map(|k| k.strip_prefix("node-role.kubernetes.io/"))
                        .collect()
                })
                .unwrap_or_default();
            (
                if ready { "Ready" } else { "NotReady" }.into(),
                !ready,
                format!(
                    "{} · {}",
                    text(object, "/status/nodeInfo/kubeletVersion").unwrap_or("?"),
                    if roles.is_empty() {
                        "worker".to_string()
                    } else {
                        roles.join(",")
                    }
                ),
            )
        }
        "Namespace" => (
            text(object, "/status/phase").unwrap_or("Active").into(),
            false,
            String::new(),
        ),
        "Event" => (
            text(object, "/type").unwrap_or("Normal").into(),
            text(object, "/type") == Some("Warning"),
            format!(
                "{}: {}",
                text(object, "/reason").unwrap_or_default(),
                text(object, "/message").unwrap_or_default()
            ),
        ),
        "Service" => (
            text(object, "/spec/type").unwrap_or("ClusterIP").into(),
            false,
            format!(
                "{} · {}",
                text(object, "/spec/clusterIP").unwrap_or("-"),
                ports(object)
            ),
        ),
        "Ingress" => {
            let hosts: Vec<&str> = object
                .pointer("/spec/rules")
                .and_then(Value::as_array)
                .map(|rules| rules.iter().filter_map(|r| text(r, "/host")).collect())
                .unwrap_or_default();
            (String::new(), false, hosts.join(", "))
        }
        "ConfigMap" => (
            String::new(),
            false,
            format!("{} keys", len(object, "/data") + len(object, "/binaryData")),
        ),
        "PersistentVolumeClaim" => {
            let phase = text(object, "/status/phase").unwrap_or("Unknown");
            (
                phase.into(),
                phase == "Lost",
                format!(
                    "{} · {}",
                    text(object, "/status/capacity/storage").unwrap_or("-"),
                    text(object, "/spec/storageClassName").unwrap_or("-")
                ),
            )
        }
        "PersistentVolume" => {
            let phase = text(object, "/status/phase").unwrap_or("Unknown");
            (
                phase.into(),
                phase == "Failed",
                format!(
                    "{} · {}",
                    text(object, "/spec/capacity/storage").unwrap_or("-"),
                    text(object, "/spec/claimRef/name").unwrap_or("-")
                ),
            )
        }
        "StorageClass" => (
            String::new(),
            false,
            text(object, "/provisioner").unwrap_or_default().to_string(),
        ),
        "RoleBinding" | "ClusterRoleBinding" => (
            String::new(),
            false,
            text(object, "/roleRef/name")
                .unwrap_or_default()
                .to_string(),
        ),
        "Role" | "ClusterRole" => (
            String::new(),
            false,
            format!("{} rules", len(object, "/rules")),
        ),
        "Certificate" => {
            let ready = object
                .pointer("/status/conditions")
                .and_then(Value::as_array)
                .and_then(|all| all.iter().find(|c| text(c, "/type") == Some("Ready")))
                .and_then(|c| text(c, "/status"))
                == Some("True");
            (
                if ready { "Ready" } else { "NotReady" }.into(),
                !ready,
                text(object, "/spec/secretName")
                    .unwrap_or_default()
                    .to_string(),
            )
        }
        _ => (String::new(), false, String::new()),
    }
}

/// A list row for `object`.
#[must_use]
pub fn entry(api_version: &str, kind: &str, object: &Value) -> Option<Entry> {
    let (status, attention, detail) = summarize(kind, object);
    Some(Entry {
        api_version: api_version.to_string(),
        kind: kind.to_string(),
        name: text(object, "/metadata/name")?.to_string(),
        namespace: text(object, "/metadata/namespace").map(str::to_string),
        status,
        detail,
        created: text(object, "/metadata/creationTimestamp").map(str::to_string),
        attention,
        usage: String::new(),
    })
}

/// An event object as a row of the list of events.
#[must_use]
pub fn event(object: &Value) -> EventInfo {
    let last = text(object, "/lastTimestamp")
        .or_else(|| text(object, "/eventTime"))
        .or_else(|| text(object, "/metadata/creationTimestamp"))
        .map(str::to_string);
    EventInfo {
        kind: text(object, "/type").unwrap_or("Normal").to_string(),
        reason: text(object, "/reason").unwrap_or_default().to_string(),
        message: text(object, "/message").unwrap_or_default().to_string(),
        count: object
            .pointer("/count")
            .and_then(Value::as_i64)
            .unwrap_or(1),
        last,
        object: format!(
            "{}/{}",
            text(object, "/involvedObject/kind").unwrap_or_default(),
            text(object, "/involvedObject/name").unwrap_or_default()
        ),
    }
}

/// What the API server adds that is noise to a person reading the YAML (not to one editing it: the status
/// stays).
pub fn clean_for_reading(value: &mut Value) {
    if let Some(meta) = value
        .as_object_mut()
        .and_then(|o| o.get_mut("metadata"))
        .and_then(Value::as_object_mut)
    {
        meta.remove("managedFields");
        if let Some(annotations) = meta.get_mut("annotations").and_then(Value::as_object_mut) {
            annotations.remove("kubectl.kubernetes.io/last-applied-configuration");
        }
    }
}

/// The names of a pod's containers, init containers first.
#[must_use]
pub fn containers(object: &Value) -> Vec<String> {
    ["/spec/initContainers", "/spec/containers"]
        .iter()
        .filter_map(|pointer| object.pointer(pointer).and_then(Value::as_array))
        .flatten()
        .filter_map(|c| text(c, "/name").map(str::to_string))
        .collect()
}

// ── usage ────────────────────────────────────────────────────────────────────

/// What a metrics-server quantity of CPU comes to, in millicores (`250m`, `1`, `12345678n`).
#[must_use]
pub fn millicores(quantity: &str) -> u64 {
    let q = quantity.trim();
    let number = |suffix: &str| q.strip_suffix(suffix).and_then(|n| n.parse::<f64>().ok());
    let value = if let Some(n) = number("n") {
        n / 1_000_000.0
    } else if let Some(u) = number("u") {
        u / 1000.0
    } else if let Some(m) = number("m") {
        m
    } else {
        q.parse::<f64>().map_or(0.0, |cores| cores * 1000.0)
    };
    value.max(0.0).round() as u64
}

/// What a quantity of memory comes to, in bytes (`512Ki`, `3Gi`, `100M`, plain bytes).
#[must_use]
pub fn bytes(quantity: &str) -> u64 {
    let q = quantity.trim();
    let units: [(&str, f64); 9] = [
        ("Ki", 1024.0),
        ("Mi", 1024.0 * 1024.0),
        ("Gi", 1024.0 * 1024.0 * 1024.0),
        ("Ti", 1024.0 * 1024.0 * 1024.0 * 1024.0),
        ("k", 1000.0),
        ("K", 1000.0),
        ("M", 1_000_000.0),
        ("G", 1_000_000_000.0),
        ("T", 1_000_000_000_000.0),
    ];
    for (suffix, factor) in units {
        if let Some(n) = q.strip_suffix(suffix).and_then(|n| n.parse::<f64>().ok()) {
            return (n * factor).max(0.0).round() as u64;
        }
    }
    q.parse::<f64>().map_or(0, |n| n.max(0.0).round() as u64)
}

/// `1.2Gi`, `340Mi`, `12Ki`: bytes as a person reads them.
#[must_use]
pub fn human_bytes(value: u64) -> String {
    let v = value as f64;
    let (unit, div) = if v >= 1024.0 * 1024.0 * 1024.0 {
        ("Gi", 1024.0 * 1024.0 * 1024.0)
    } else if v >= 1024.0 * 1024.0 {
        ("Mi", 1024.0 * 1024.0)
    } else {
        ("Ki", 1024.0)
    };
    let n = v / div;
    if n >= 100.0 || unit == "Ki" {
        format!("{n:.0}{unit}")
    } else {
        format!("{n:.1}{unit}")
    }
}

/// `(namespace, name)` -> (millicores, bytes) from a metrics-server list of nodes or pods.
#[must_use]
pub fn usage_of(list: &Value) -> std::collections::BTreeMap<(String, String), (u64, u64)> {
    let mut found = std::collections::BTreeMap::new();
    for item in list
        .get("items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(name) = text(item, "/metadata/name") else {
            continue;
        };
        let namespace = text(item, "/metadata/namespace").unwrap_or_default();
        let (cpu, memory) = match item.get("containers").and_then(Value::as_array) {
            Some(containers) => containers.iter().fold((0, 0), |(c, m), container| {
                (
                    c + millicores(text(container, "/usage/cpu").unwrap_or_default()),
                    m + bytes(text(container, "/usage/memory").unwrap_or_default()),
                )
            }),
            None => (
                millicores(text(item, "/usage/cpu").unwrap_or_default()),
                bytes(text(item, "/usage/memory").unwrap_or_default()),
            ),
        };
        found.insert((namespace.to_string(), name.to_string()), (cpu, memory));
    }
    found
}

/// Writes each entry's usage from `usage`.
pub fn add_usage(
    entries: &mut [Entry],
    usage: &std::collections::BTreeMap<(String, String), (u64, u64)>,
) {
    for entry in entries {
        let key = (
            entry.namespace.clone().unwrap_or_default(),
            entry.name.clone(),
        );
        if let Some((cpu, memory)) = usage.get(&key) {
            entry.usage = format!("{cpu}m · {}", human_bytes(*memory));
        }
    }
}

/// The label selector (`app=web,tier=api`) of the pods something owns or selects: a workload's
/// `spec.selector.matchLabels`, a service's `spec.selector`.
#[must_use]
pub fn pod_selector(kind: &str, object: &Value) -> Option<String> {
    let labels = match kind {
        "Service" => object.pointer("/spec/selector"),
        "Deployment" | "StatefulSet" | "DaemonSet" | "ReplicaSet" | "Job" => {
            object.pointer("/spec/selector/matchLabels")
        }
        _ => None,
    }?
    .as_object()?;
    let parts: Vec<String> = labels
        .iter()
        .filter_map(|(k, v)| v.as_str().map(|v| format!("{k}={v}")))
        .collect();
    (!parts.is_empty()).then(|| parts.join(","))
}

// ── overview ─────────────────────────────────────────────────────────────────

/// The cluster in a few numbers.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Overview {
    pub nodes_ready: usize,
    pub nodes_total: usize,
    pub cpu_used_m: u64,
    pub cpu_total_m: u64,
    pub memory_used: u64,
    pub memory_total: u64,
    pub pods_running: usize,
    pub pods_total: usize,
    /// Pods that need attention (crash loops, failed), by name.
    pub pods_attention: Vec<Entry>,
    /// The latest warnings.
    pub warnings: Vec<EventInfo>,
    /// Whether metrics-server answered, so usage is real and not zero.
    pub metrics: bool,
}

/// Adds up nodes, pods and events (and the usage of the nodes, when metrics-server gave it).
#[must_use]
pub fn overview(
    nodes: &[Value],
    node_usage: Option<&Value>,
    pods: &[Value],
    events: &[Value],
) -> Overview {
    let mut out = Overview {
        nodes_total: nodes.len(),
        ..Overview::default()
    };
    for node in nodes {
        if !summarize("Node", node).1 {
            out.nodes_ready += 1;
        }
        out.cpu_total_m += millicores(text(node, "/status/allocatable/cpu").unwrap_or_default());
        out.memory_total += bytes(text(node, "/status/allocatable/memory").unwrap_or_default());
    }
    if let Some(usage) = node_usage {
        out.metrics = true;
        for (cpu, memory) in usage_of(usage).values() {
            out.cpu_used_m += cpu;
            out.memory_used += memory;
        }
    }
    out.pods_total = pods.len();
    for pod in pods {
        let (status, bad, _) = summarize("Pod", pod);
        if status == "Running" || status == "Succeeded" {
            out.pods_running += 1;
        }
        if bad && let Some(entry) = entry("v1", "Pod", pod) {
            out.pods_attention.push(entry);
        }
    }
    let mut warnings: Vec<EventInfo> = events
        .iter()
        .filter(|e| text(e, "/type") == Some("Warning"))
        .map(event)
        .collect();
    warnings.sort_by(|a, b| b.last.cmp(&a.last));
    warnings.truncate(10);
    out.warnings = warnings;
    out
}

// ── facade ───────────────────────────────────────────────────────────────────

/// A place the cluster serves, as a link: what Palantir's front page was.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FacadeEntry {
    pub name: String,
    pub namespace: String,
    pub url: String,
    /// Whether something is running behind it.
    pub available: bool,
}

/// Whether a pod's labels satisfy a service's selector (an empty selector selects nothing).
fn selected(selector: &Value, pod: &Value) -> bool {
    let Some(wanted) = selector.as_object().filter(|s| !s.is_empty()) else {
        return false;
    };
    let labels = pod.pointer("/metadata/labels");
    wanted.iter().all(|(key, value)| {
        labels.and_then(|l| l.get(key)).and_then(Value::as_str) == value.as_str()
    })
}

/// One entry per host and path an Ingress serves, available when a pod of the service behind it runs.
#[must_use]
pub fn facade(ingresses: &[Value], services: &[Value], pods: &[Value]) -> Vec<FacadeEntry> {
    let mut found = Vec::new();
    for ingress in ingresses {
        let namespace = text(ingress, "/metadata/namespace").unwrap_or_default();
        let name = text(ingress, "/metadata/name").unwrap_or_default();
        let tls_hosts: Vec<&str> = ingress
            .pointer("/spec/tls")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .flat_map(|t| {
                t.get("hosts")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
            })
            .filter_map(Value::as_str)
            .collect();
        for rule in ingress
            .pointer("/spec/rules")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let Some(host) = text(rule, "/host") else {
                continue;
            };
            let scheme = if tls_hosts.contains(&host) {
                "https"
            } else {
                "http"
            };
            for path in rule
                .pointer("/http/paths")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let service = text(path, "/backend/service/name").unwrap_or_default();
                let selector = services
                    .iter()
                    .find(|s| {
                        text(s, "/metadata/name") == Some(service)
                            && text(s, "/metadata/namespace") == Some(namespace)
                    })
                    .and_then(|s| s.pointer("/spec/selector"));
                let available = selector.is_some_and(|selector| {
                    pods.iter().any(|pod| {
                        text(pod, "/metadata/namespace") == Some(namespace)
                            && text(pod, "/status/phase") == Some("Running")
                            && selected(selector, pod)
                    })
                });
                let path_text = text(path, "/path").unwrap_or("/");
                found.push(FacadeEntry {
                    name: name.to_string(),
                    namespace: namespace.to_string(),
                    url: format!("{scheme}://{host}{path_text}"),
                    available,
                });
            }
        }
    }
    let address = |e: &FacadeEntry| {
        e.url
            .split_once("://")
            .map_or_else(|| e.url.clone(), |(_, rest)| rest.to_string())
    };
    found.sort_by(|a, b| (address(a), &a.name).cmp(&(address(b), &b.name)));
    found.dedup();
    found
}
