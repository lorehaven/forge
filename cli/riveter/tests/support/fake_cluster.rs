//! A Kubernetes API server in a thread, for tests: it keeps what is applied to it, answers discovery for a handful of kinds,
//! and records the calls made. Riveter reaches it the way it reaches a real cluster - through a kubeconfig that
//! `KUBECONFIG` names.
//!
//! It plays a cluster that is quick and agreeable: a Deployment is rolled out the moment it is applied, a Job is in the state
//! the test chose, and what it is told to refuse it refuses with a `Status`.

#![allow(
    dead_code,
    clippy::type_complexity,
    clippy::missing_const_for_fn,
    clippy::format_push_string,
    clippy::too_many_lines,
    clippy::cmp_owned,
    reason = "a test double: each test crate that includes it uses some of it, and it favours being short over being tidy"
)]

use quench_serde::{Value, json};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

/// What the Jobs of a cluster report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobMode {
    Running,
    Complete,
    Failed(String),
}

/// How the cluster answers when asked whether a namespace exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NamespaceMode {
    /// It exists once something is applied into it, and `with_namespace` creates it up front.
    Normal,
    /// The question is answered with a server error.
    Unreachable,
}

#[derive(Debug)]
struct State {
    /// `plural/namespace/name` to the object.
    objects: BTreeMap<String, Value>,
    calls: Vec<String>,
    /// When, in the order of everything every fake cluster was asked, the first call came.
    first_at: Option<u64>,
    job: JobMode,
    namespaces: NamespaceMode,
    /// An apply of an object with this `kind/name` is refused.
    refuse: Option<String>,
    refuse_all: bool,
}

pub struct FakeCluster {
    port: u16,
    state: Arc<Mutex<State>>,
    version: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
}

const CORE: &[(&str, &str, bool)] = &[
    ("namespaces", "Namespace", false),
    ("pods", "Pod", true),
    ("services", "Service", true),
    ("endpoints", "Endpoints", true),
    ("configmaps", "ConfigMap", true),
    ("secrets", "Secret", true),
];
const APPS: &[(&str, &str, bool)] = &[
    ("deployments", "Deployment", true),
    ("replicasets", "ReplicaSet", true),
    ("statefulsets", "StatefulSet", true),
    ("daemonsets", "DaemonSet", true),
];
const BATCH: &[(&str, &str, bool)] = &[("jobs", "Job", true), ("cronjobs", "CronJob", true)];
const EXTENSIONS: &[(&str, &str, bool)] = &[(
    "customresourcedefinitions",
    "CustomResourceDefinition",
    false,
)];

fn groups() -> [(&'static str, &'static [(&'static str, &'static str, bool)]); 4] {
    [
        ("", CORE),
        ("apps", APPS),
        ("batch", BATCH),
        ("apiextensions.k8s.io", EXTENSIONS),
    ]
}

impl FakeCluster {
    /// A cluster that is running, with nothing in it.
    pub fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let state = Arc::new(Mutex::new(State {
            objects: BTreeMap::new(),
            calls: Vec::new(),
            first_at: None,
            job: JobMode::Complete,
            namespaces: NamespaceMode::Normal,
            refuse: None,
            refuse_all: false,
        }));
        let version = Arc::new(AtomicU64::new(100));
        let stop = Arc::new(AtomicBool::new(false));
        {
            let (state, version, stop) = (state.clone(), version.clone(), stop.clone());
            std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            let (state, version) = (state.clone(), version.clone());
                            std::thread::spawn(move || serve(stream, &state, &version));
                        }
                        Err(_) => std::thread::sleep(std::time::Duration::from_millis(5)),
                    }
                }
            });
        }
        Self {
            port,
            state,
            version,
            stop,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn server(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// A kubeconfig with one context per cluster, the first being the current one. Returns its path.
    pub fn kubeconfig(dir: &std::path::Path, contexts: &[(&str, &Self)]) -> std::path::PathBuf {
        let mut text = String::from("apiVersion: v1\nkind: Config\n");
        text.push_str(&format!("current-context: {}\n", contexts[0].0));
        text.push_str("clusters:\n");
        for (name, cluster) in contexts {
            text.push_str(&format!(
                "  - name: {name}\n    cluster:\n      server: {}\n",
                cluster.server()
            ));
        }
        text.push_str("users:\n  - name: tester\n    user: {}\ncontexts:\n");
        for (name, _) in contexts {
            text.push_str(&format!(
                "  - name: {name}\n    context:\n      cluster: {name}\n      user: tester\n      namespace: default\n"
            ));
        }
        let path = dir.join("kubeconfig");
        std::fs::write(&path, text).unwrap();
        path
    }

    pub fn job_is(&self, mode: JobMode) -> &Self {
        self.lock().job = mode;
        self
    }

    pub fn namespaces_are(&self, mode: NamespaceMode) -> &Self {
        self.lock().namespaces = mode;
        self
    }

    /// Refuses to apply the object `kind/name` (lowercase kind).
    pub fn refuses(&self, object: &str) -> &Self {
        self.lock().refuse = Some(object.to_string());
        self
    }

    /// Refuses to apply anything.
    pub fn refuses_everything(&self) -> &Self {
        self.lock().refuse_all = true;
        self
    }

    /// Puts an object into the cluster as it is, as if something else had made it.
    pub fn insert(&self, plural: &str, object: &str) -> &Self {
        let value: Value = json::from_str(object).unwrap();
        let name = value["metadata"]["name"].as_str().unwrap().to_string();
        let namespace = value["metadata"]["namespace"]
            .as_str()
            .unwrap_or("")
            .to_string();
        self.lock()
            .objects
            .insert(format!("{plural}/{namespace}/{name}"), value);
        self
    }

    pub fn has(&self, plural: &str, namespace: &str, name: &str) -> bool {
        self.lock()
            .objects
            .contains_key(&format!("{plural}/{namespace}/{name}"))
    }

    pub fn object(&self, plural: &str, namespace: &str, name: &str) -> Option<Value> {
        self.lock()
            .objects
            .get(&format!("{plural}/{namespace}/{name}"))
            .cloned()
    }

    /// The calls made, in order, as `apply deployments/db`, `dry-run ...`, `get ...`, `list ...`, `delete ...`. Discovery and
    /// questions about namespaces are left out.
    pub fn calls(&self) -> Vec<String> {
        self.lock().calls.clone()
    }

    /// When this cluster was first asked anything, to compare with another's: `None` if never.
    pub fn first_asked(&self) -> Option<u64> {
        self.lock().first_at
    }

    pub fn forget_calls(&self) {
        let mut state = self.lock();
        state.calls.clear();
        state.first_at = None;
    }

    pub fn versions_used(&self) -> u64 {
        self.version.load(Ordering::Relaxed)
    }
}

impl Drop for FakeCluster {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

// ── The server ───────────────────────────────────────────────────────────────────────────────────────────────────────

struct Request {
    method: String,
    path: String,
    query: Vec<(String, String)>,
    body: Vec<u8>,
    content_type: String,
}

fn read_request(stream: &mut TcpStream) -> Option<Request> {
    stream.set_nonblocking(false).ok()?;
    let mut data = Vec::new();
    let mut chunk = [0_u8; 4096];
    let head_end = loop {
        if let Some(at) = data.windows(4).position(|w| w == b"\r\n\r\n") {
            break at;
        }
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            return None;
        }
        data.extend_from_slice(&chunk[..n]);
    };
    let head = String::from_utf8_lossy(&data[..head_end]).to_string();
    let mut lines = head.lines();
    let mut first = lines.next()?.split(' ');
    let method = first.next()?.to_string();
    let target = first.next()?.to_string();
    let headers: Vec<(&str, &str)> = lines.filter_map(|l| l.split_once(':')).collect();
    let header = |name: &str| {
        headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map_or("", |(_, v)| v.trim())
            .to_string()
    };
    let length = header("content-length").parse::<usize>().unwrap_or(0);
    let content_type = header("content-type");
    let mut body = data[head_end + 4..].to_vec();
    while body.len() < length {
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..n]);
    }
    let (path, query) = target.split_once('?').unwrap_or((&target, ""));
    let query = query
        .split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap_or((p, ""));
            (decode(k), decode(v))
        })
        .collect();
    Some(Request {
        method,
        path: path.to_string(),
        query,
        body,
        content_type,
    })
}

fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
                if let Ok(byte) = u8::from_str_radix(hex, 16) {
                    out.push(byte);
                    i += 3;
                } else {
                    out.push(b'%');
                    i += 1;
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            other => {
                out.push(other);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).to_string()
}

fn respond(stream: &mut TcpStream, code: u16, body: &str) {
    let reason = match code {
        200 => "OK",
        201 => "Created",
        404 => "Not Found",
        _ => "Error",
    };
    let _ = write!(
        stream,
        "HTTP/1.1 {code} {reason}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    );
}

fn status(code: u16, reason: &str, message: &str) -> (u16, String) {
    (
        code,
        format!(
            r#"{{"kind":"Status","apiVersion":"v1","status":"Failure","reason":"{reason}","message":"{}","code":{code}}}"#,
            message.replace('"', "'")
        ),
    )
}

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Records a call, and when it came relative to every other fake cluster's.
fn note(state: &mut State, call: String) {
    state
        .first_at
        .get_or_insert_with(|| SEQUENCE.fetch_add(1, Ordering::SeqCst));
    state.calls.push(call);
}

fn serve(mut stream: TcpStream, state: &Mutex<State>, version: &AtomicU64) {
    let Some(request) = read_request(&mut stream) else {
        return;
    };
    let (code, body) = {
        let mut state = state.lock().unwrap_or_else(PoisonError::into_inner);
        handle(&request, &mut state, version)
    };
    respond(&mut stream, code, &body);
}

/// The resources of a group version, as discovery lists them.
fn resource_list(group: &str, kinds: &[(&str, &str, bool)]) -> String {
    let api_version = if group.is_empty() {
        "v1".to_string()
    } else {
        format!("{group}/v1")
    };
    let items: Vec<String> = kinds
        .iter()
        .map(|(plural, kind, namespaced)| {
            format!(
                r#"{{"name":"{plural}","kind":"{kind}","namespaced":{namespaced},"verbs":["get","list","create","patch","delete"]}}"#
            )
        })
        .collect();
    format!(
        r#"{{"kind":"APIResourceList","groupVersion":"{api_version}","resources":[{}]}}"#,
        items.join(",")
    )
}

/// The kind a plural stands for.
fn kind_of(plural: &str) -> Option<&'static str> {
    groups()
        .iter()
        .flat_map(|(_, kinds)| kinds.iter())
        .find(|(p, _, _)| *p == plural)
        .map(|(_, kind, _)| *kind)
}

/// `labelSelector=a=b,c=d` against an object's labels.
fn selected(object: &Value, selector: &str) -> bool {
    selector.split(',').filter(|s| !s.is_empty()).all(|pair| {
        pair.split_once('=')
            .is_some_and(|(key, wanted)| object["metadata"]["labels"][key].as_str() == Some(wanted))
    })
}

fn handle(request: &Request, state: &mut State, version: &AtomicU64) -> (u16, String) {
    let path = request.path.as_str();

    if path == "/apis" {
        let names: Vec<String> = groups()
            .iter()
            .filter(|(group, _)| !group.is_empty())
            .map(|(group, _)| {
                format!(
                    r#"{{"name":"{group}","versions":[{{"groupVersion":"{group}/v1","version":"v1"}}],"preferredVersion":{{"groupVersion":"{group}/v1","version":"v1"}}}}"#
                )
            })
            .collect();
        return (
            200,
            format!(
                r#"{{"kind":"APIGroupList","groups":[{}]}}"#,
                names.join(",")
            ),
        );
    }
    for (group, kinds) in groups() {
        let listing = if group.is_empty() {
            "/api/v1".to_string()
        } else {
            format!("/apis/{group}/v1")
        };
        if path == listing {
            return (200, resource_list(group, kinds));
        }
    }

    // `/api/v1/...` or `/apis/<group>/v1/...`, then `[namespaces/<ns>/]<plural>[/<name>]`.
    let rest = if let Some(rest) = path.strip_prefix("/api/v1/") {
        rest
    } else if let Some(rest) = path.strip_prefix("/apis/") {
        let mut parts = rest.splitn(3, '/');
        let _group = parts.next();
        let _version = parts.next();
        parts.next().unwrap_or("")
    } else {
        return status(404, "NotFound", "no such path");
    };
    let segments: Vec<&str> = rest.split('/').collect();
    let (namespace, plural, name) = if segments.len() >= 3 && segments[0] == "namespaces" {
        (segments[1], segments[2], segments.get(3).copied())
    } else {
        ("", segments[0], segments.get(1).copied())
    };
    let Some(kind) = kind_of(plural) else {
        return status(
            404,
            "NotFound",
            "the server could not find the requested resource",
        );
    };
    let key = |name: &str| format!("{plural}/{namespace}/{name}");

    match (request.method.as_str(), name) {
        ("GET", Some(name)) => {
            if plural == "namespaces" {
                if state.namespaces == NamespaceMode::Unreachable {
                    return status(500, "InternalError", "the server is unwell");
                }
            } else {
                note(state, format!("get {plural}/{name}"));
            }
            match state.objects.get(&key(name)).cloned() {
                Some(mut object) => {
                    if kind == "Job" {
                        add_job_status(&mut object, &state.job);
                    }
                    (200, object.to_string())
                }
                None if plural == "namespaces"
                    && state
                        .objects
                        .keys()
                        .any(|k| k.split('/').nth(1) == Some(name)) =>
                {
                    (
                        200,
                        format!(
                            r#"{{"apiVersion":"v1","kind":"Namespace","metadata":{{"name":"{name}"}}}}"#
                        ),
                    )
                }
                None => status(404, "NotFound", &format!("{plural} \"{name}\" not found")),
            }
        }
        ("GET", None) => {
            note(state, format!("list {plural}"));
            let selector = request
                .query
                .iter()
                .find(|(k, _)| k == "labelSelector")
                .map_or("", |(_, v)| v.as_str());
            let prefix = format!("{plural}/{namespace}/");
            let items: Vec<String> = state
                .objects
                .iter()
                .filter(|(k, _)| {
                    k.starts_with(&prefix)
                        || (namespace.is_empty() && k.starts_with(&format!("{plural}/")))
                })
                .filter(|(_, object)| selected(object, selector))
                .map(|(_, object)| object.to_string())
                .collect();
            (
                200,
                format!(
                    r#"{{"kind":"{kind}List","apiVersion":"v1","metadata":{{}},"items":[{}]}}"#,
                    items.join(",")
                ),
            )
        }
        ("PATCH", Some(name)) => {
            let dry = request.query.iter().any(|(k, _)| k == "dryRun");
            let mut object: Value = match json::from_str(&String::from_utf8_lossy(&request.body)) {
                Ok(object) => object,
                Err(_) => return status(400, "BadRequest", "the body is not JSON"),
            };
            // A merge or JSON patch changes what is there; an apply is the whole object.
            if request.content_type.contains("merge-patch")
                || request.content_type.contains("json-patch")
            {
                let Some(mut live) = state.objects.get(&key(name)).cloned() else {
                    return status(404, "NotFound", &format!("{plural} \"{name}\" not found"));
                };
                if request.content_type.contains("merge-patch") {
                    merge(&mut live, &object);
                } else {
                    for op in object.as_array().cloned().unwrap_or_default() {
                        let path = op["path"].as_str().unwrap_or("").to_string();
                        let mut at = &mut live;
                        let parts: Vec<&str> = path.trim_start_matches('/').split('/').collect();
                        for part in &parts[..parts.len() - 1] {
                            at = &mut at[*part];
                        }
                        if let Some(map) = at.as_object_mut() {
                            map.insert(parts[parts.len() - 1], op["value"].clone());
                        }
                    }
                }
                note(state, format!("patch {plural}/{name}"));
                let next = version.fetch_add(1, Ordering::Relaxed).to_string();
                if let Some(metadata) = live.get_mut("metadata").and_then(Value::as_object_mut) {
                    metadata.insert("resourceVersion", Value::String(next));
                }
                state.objects.insert(key(name), live.clone());
                return (200, live.to_string());
            }
            let kind_name = format!("{}/{name}", kind.to_lowercase());
            if state.refuse_all || state.refuse.as_deref() == Some(kind_name.as_str()) {
                return status(
                    500,
                    "InternalError",
                    &format!("error from the server: {kind_name} refused"),
                );
            }
            note(
                state,
                format!("{} {plural}/{name}", if dry { "dry-run" } else { "apply" }),
            );
            let before = state.objects.get(&key(name)).cloned();
            settle(&mut object, kind);
            let unchanged = before.as_ref().is_some_and(|before| {
                strip(before.clone()).to_string() == strip(object.clone()).to_string()
            });
            let resource_version = match (&before, unchanged) {
                (Some(before), true) => before["metadata"]["resourceVersion"]
                    .as_str()
                    .unwrap_or("1")
                    .to_string(),
                _ if dry => before
                    .as_ref()
                    .and_then(|b| b["metadata"]["resourceVersion"].as_str())
                    .unwrap_or("1")
                    .to_string(),
                _ => version.fetch_add(1, Ordering::Relaxed).to_string(),
            };
            if let Some(metadata) = object.get_mut("metadata").and_then(Value::as_object_mut) {
                metadata.insert("resourceVersion", Value::String(resource_version));
            }
            if !dry {
                state.objects.insert(key(name), object.clone());
            }
            (200, object.to_string())
        }
        ("DELETE", Some(name)) => {
            note(state, format!("delete {plural}/{name}"));
            match state.objects.remove(&key(name)) {
                Some(_) => (
                    200,
                    r#"{"kind":"Status","apiVersion":"v1","status":"Success"}"#.to_string(),
                ),
                None => status(404, "NotFound", &format!("{plural} \"{name}\" not found")),
            }
        }
        _ => status(405, "MethodNotAllowed", "not supported by the fake"),
    }
}

/// RFC 7386: maps merge, anything else is replaced, `null` removes.
fn merge(into: &mut Value, patch: &Value) {
    let (Some(target), Some(changes)) = (into.as_object_mut(), patch.as_object()) else {
        *into = patch.clone();
        return;
    };
    for (key, change) in changes.iter() {
        if change.is_null() {
            target.remove(key);
        } else if let Some(existing) = target.get_mut(key) {
            merge(existing, change);
        } else {
            target.insert(key.clone(), change.clone());
        }
    }
}

/// The object as it is compared for "did applying change anything": without what the server stamps.
fn strip(mut object: Value) -> Value {
    if let Some(metadata) = object.get_mut("metadata").and_then(Value::as_object_mut) {
        metadata.remove("resourceVersion");
    }
    object
}

/// What a controller does to an object a moment after it is applied: a workload has been seen and rolled out in full.
fn settle(object: &mut Value, kind: &str) {
    if !matches!(kind, "Deployment" | "StatefulSet") {
        return;
    }
    let replicas = object["spec"]["replicas"].as_i64().unwrap_or(1);
    let status = format!(
        r#"{{"observedGeneration":1,"replicas":{replicas},"updatedReplicas":{replicas},"availableReplicas":{replicas},"readyReplicas":{replicas}}}"#
    );
    if let (Some(map), Ok(status)) = (object.as_object_mut(), json::from_str::<Value>(&status)) {
        map.insert("status", status);
    }
    if let Some(metadata) = object.get_mut("metadata").and_then(Value::as_object_mut) {
        metadata.insert("generation", Value::from(1_i64));
    }
}

fn add_job_status(job: &mut Value, mode: &JobMode) {
    let conditions = match mode {
        JobMode::Running => "[]".to_string(),
        JobMode::Complete => r#"[{"type":"Complete","status":"True"}]"#.to_string(),
        JobMode::Failed(message) => format!(
            r#"[{{"type":"Failed","status":"True","message":"{}"}}]"#,
            message.replace('"', "'")
        ),
    };
    let status = format!(r#"{{"conditions":{conditions}}}"#);
    if let (Some(map), Ok(status)) = (job.as_object_mut(), json::from_str::<Value>(&status)) {
        map.insert("status", status);
    }
}

// ── Reaching it ──────────────────────────────────────────────────────────────────────────────────────────────────────

static KUBECONFIG_LOCK: Mutex<()> = Mutex::new(());

/// The lock on `KUBECONFIG`, for tests of other kinds that set it too.
pub fn kubeconfig_lock() -> std::sync::MutexGuard<'static, ()> {
    KUBECONFIG_LOCK
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

/// Runs `body` with `KUBECONFIG` naming a kubeconfig of these contexts (the first is the current one). The variable is the
/// process's, so one body at a time.
pub fn with_clusters<T>(contexts: &[(&str, &FakeCluster)], body: impl FnOnce() -> T) -> T {
    let _guard = KUBECONFIG_LOCK
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    let dir = quench_core::tmp::TempDir::new().unwrap();
    let path = FakeCluster::kubeconfig(dir.path(), contexts);
    let original = quench_core::env::var("KUBECONFIG");
    quench_core::env::set("KUBECONFIG", &path);
    let result = body();
    match original {
        Some(original) => quench_core::env::set("KUBECONFIG", original),
        None => quench_core::env::remove("KUBECONFIG"),
    }
    result
}

/// [`with_clusters`] for the usual one cluster.
pub fn with_cluster<T>(cluster: &FakeCluster, body: impl FnOnce() -> T) -> T {
    with_clusters(&[("test", cluster)], body)
}
