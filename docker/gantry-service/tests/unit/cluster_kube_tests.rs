//! `KubeCluster`, the real `Cluster`, against a fake Kubernetes API: a mock server that serves the
//! discovery documents and the few object paths Gantry reads, with a kubeconfig that points at it.

use gantry_service::domain::cluster::{
    Cluster, JobSpec, JobStatus, KubeCluster, LiveResource, Workload,
};
use gantry_service::domain::settings::{ClusterMode, Settings};
use quench_test::mock::matchers::{method, path};
use quench_test::mock::{Mock, MockServer, ResponseTemplate};
use std::sync::{Mutex, MutexGuard, OnceLock};

/// `KUBECONFIG` is process-global.
fn env_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn json(status: u16, body: &quench_serde::Value) -> ResponseTemplate {
    ResponseTemplate::new(status)
        .set_body_raw(quench_serde::json::to_string(body), "application/json")
}

/// A fake API server and a `KubeCluster` that reads its kubeconfig.
async fn connect() -> (MockServer, KubeCluster, quench_core::tmp::TempDir) {
    let server = MockServer::start().await;
    let dir = quench_core::tmp::TempDir::new().unwrap();
    let kubeconfig = dir.path().join("kubeconfig");
    std::fs::write(
        &kubeconfig,
        format!(
            "apiVersion: v1
kind: Config
current-context: fake
contexts:
  - name: fake
    context: {{cluster: fake, user: fake, namespace: gantry}}
clusters:
  - name: fake
    cluster: {{server: '{}'}}
users:
  - name: fake
    user: {{token: secret-token}}
",
            server.uri()
        ),
    )
    .unwrap();
    quench_core::env::set("KUBECONFIG", kubeconfig.display().to_string());
    let mut settings = Settings::inert();
    settings.cluster = ClusterMode::Kubeconfig;
    let cluster = KubeCluster::connect(&settings).expect("a cluster");
    (server, cluster, dir)
}

async fn serve(server: &MockServer, verb: &str, route: &str, response: ResponseTemplate) {
    Mock::given(method(verb))
        .and(path(route))
        .respond_with(response)
        .mount(server)
        .await;
}

fn spec() -> JobSpec {
    JobSpec {
        namespace: "gantry".into(),
        name: "op-1".into(),
        image: "registry.example/runner:1".into(),
        service_account: "runner".into(),
        plan_json: "{}".into(),
        allowed_namespaces: vec!["demo".into()],
        credentials_secret: "creds".into(),
        age_key_secret: "age".into(),
        values_secrets: vec!["gantry-values-demo".into()],
        operation_id: "op-1".into(),
        deadline_secs: 600,
    }
}

const JOB_PATH: &str = "/apis/batch/v1/namespaces/gantry/jobs";

#[allow(clippy::await_holding_lock)] // KUBECONFIG is process-global; held for the whole test
#[quench_test::test]
async fn a_runner_job_is_created_and_a_repeat_is_not_an_error() {
    let _env = env_lock();
    let (server, cluster, _dir) = connect().await;
    serve(
        &server,
        "POST",
        JOB_PATH,
        json(201, &quench_serde::json!({"metadata": {"name": "op-1"}})),
    )
    .await;
    cluster.create_job(&spec()).await.unwrap();

    let sent = server.received_requests().await.unwrap();
    let body: quench_serde::Value = quench_serde::json::from_slice(&sent[0].body).unwrap();
    assert_eq!(body["kind"], "Job");
    assert_eq!(body["metadata"]["name"], "op-1");
    assert_eq!(sent[0].header("authorization"), Some("Bearer secret-token"));

    // the same Job again: a restarted service re-issues it
    server.reset().await;
    serve(
        &server,
        "POST",
        JOB_PATH,
        json(409, &quench_serde::json!({"reason": "AlreadyExists"})),
    )
    .await;
    cluster.create_job(&spec()).await.unwrap();

    // anything else is an error that says so
    server.reset().await;
    serve(
        &server,
        "POST",
        JOB_PATH,
        json(500, &quench_serde::json!({"message": "boom"})),
    )
    .await;
    let error = cluster.create_job(&spec()).await.unwrap_err();
    assert!(error.to_string().starts_with("kubernetes:"), "{error}");
}

#[allow(clippy::await_holding_lock)]
#[quench_test::test]
async fn a_jobs_status_is_read_from_its_counters_and_conditions() {
    let _env = env_lock();
    let (server, cluster, _dir) = connect().await;
    let job = |name: &str| format!("{JOB_PATH}/{name}");

    serve(
        &server,
        "GET",
        &job("absent"),
        json(404, &quench_serde::json!({"reason": "NotFound"})),
    )
    .await;
    serve(
        &server,
        "GET",
        &job("fresh"),
        json(200, &quench_serde::json!({"metadata": {"name": "fresh"}})),
    )
    .await;
    serve(
        &server,
        "GET",
        &job("done"),
        json(200, &quench_serde::json!({"status": {"succeeded": 1}})),
    )
    .await;
    serve(
        &server,
        "GET",
        &job("broken"),
        json(
            200,
            &quench_serde::json!({"status": {"conditions": [
                {"type": "Suspended", "status": "False"},
                {"type": "Failed", "status": "True", "reason": "BackoffLimitExceeded", "message": "step 2 failed"}
            ]}}),
        ),
    )
    .await;
    serve(
        &server,
        "GET",
        &job("reasonless"),
        json(
            200,
            &quench_serde::json!({"status": {"conditions": [
                {"type": "Failed", "status": "True", "reason": "DeadlineExceeded"}
            ]}}),
        ),
    )
    .await;
    serve(
        &server,
        "GET",
        &job("nameless"),
        json(200, &quench_serde::json!({"status": {"conditions": [{"type": "Failed", "status": "True"}]}})),
    )
    .await;
    serve(
        &server,
        "GET",
        &job("running"),
        json(200, &quench_serde::json!({"status": {"active": 1, "conditions": [{"type": "Failed", "status": "False"}]}})),
    )
    .await;
    serve(&server, "GET", &job("down"), ResponseTemplate::new(500)).await;

    let status = |name: &'static str| {
        let cluster = &cluster;
        async move { cluster.job_status("gantry", name).await }
    };
    assert_eq!(status("absent").await.unwrap(), JobStatus::Absent);
    assert_eq!(status("fresh").await.unwrap(), JobStatus::Active);
    assert_eq!(status("done").await.unwrap(), JobStatus::Succeeded);
    assert_eq!(
        status("broken").await.unwrap(),
        JobStatus::Failed("step 2 failed".into())
    );
    assert_eq!(
        status("reasonless").await.unwrap(),
        JobStatus::Failed("DeadlineExceeded".into())
    );
    assert_eq!(
        status("nameless").await.unwrap(),
        JobStatus::Failed("the Job failed".into())
    );
    assert_eq!(status("running").await.unwrap(), JobStatus::Active);
    assert!(status("down").await.is_err());
}

#[allow(clippy::await_holding_lock)]
#[quench_test::test]
async fn a_jobs_log_comes_from_its_newest_pod() {
    let _env = env_lock();
    let (server, cluster, _dir) = connect().await;
    let pods = "/api/v1/namespaces/gantry/pods";
    serve(
        &server,
        "GET",
        pods,
        json(
            200,
            &quench_serde::json!({"items": [
                {"metadata": {"name": "op-1-old", "creationTimestamp": "2026-01-01T00:00:00Z"}},
                {"metadata": {"name": "op-1-new", "creationTimestamp": "2026-01-01T00:05:00Z"}},
            ]}),
        ),
    )
    .await;
    serve(
        &server,
        "GET",
        &format!("{pods}/op-1-new/log"),
        ResponseTemplate::new(200).set_body_raw("step 1 ok\nstep 2 ok\n".to_string(), "text/plain"),
    )
    .await;
    assert_eq!(
        cluster.job_log("gantry", "op-1").await.unwrap(),
        "step 1 ok\nstep 2 ok\n"
    );

    // the pods were asked for by the Job's label
    let asked = server.received_requests().await.unwrap();
    assert!(
        asked[0].query.contains("job-name%3Dop-1") || asked[0].query.contains("job-name=op-1"),
        "{}",
        asked[0].query
    );

    // a Job with no pod yet has no log
    server.reset().await;
    serve(
        &server,
        "GET",
        pods,
        json(200, &quench_serde::json!({"items": []})),
    )
    .await;
    assert_eq!(cluster.job_log("gantry", "op-2").await.unwrap(), "");

    server.reset().await;
    serve(&server, "GET", pods, ResponseTemplate::new(403)).await;
    assert!(cluster.job_log("gantry", "op-3").await.is_err());
}

#[allow(clippy::await_holding_lock)]
#[quench_test::test]
async fn deleting_a_job_that_is_gone_is_fine() {
    let _env = env_lock();
    let (server, cluster, _dir) = connect().await;
    serve(
        &server,
        "DELETE",
        &format!("{JOB_PATH}/op-1"),
        json(200, &quench_serde::json!({})),
    )
    .await;
    serve(
        &server,
        "DELETE",
        &format!("{JOB_PATH}/gone"),
        json(404, &quench_serde::json!({"reason": "NotFound"})),
    )
    .await;
    serve(
        &server,
        "DELETE",
        &format!("{JOB_PATH}/stuck"),
        ResponseTemplate::new(500),
    )
    .await;
    cluster.delete_job("gantry", "op-1").await.unwrap();
    cluster.delete_job("gantry", "gone").await.unwrap();
    assert!(cluster.delete_job("gantry", "stuck").await.is_err());
}

fn workload_object(
    name: &str,
    namespace: &str,
    package: Option<&str>,
    spec: &quench_serde::Value,
) -> quench_serde::Value {
    let mut labels = quench_serde::Map::new();
    if let Some(package) = package {
        labels.insert(
            "riveter.forge/package".to_string(),
            quench_serde::json!(package),
        );
    }
    quench_serde::json!({
        "metadata": {
            "name": name,
            "namespace": namespace,
            "labels": labels,
            "annotations": {"riveter.forge/package-version": "1.2.0+b3"},
        },
        "spec": spec["spec"].clone(),
        "status": spec["status"].clone(),
    })
}

#[allow(clippy::await_holding_lock)]
#[quench_test::test]
async fn workloads_are_listed_per_namespace_with_their_replica_counts() {
    let _env = env_lock();
    let (server, cluster, _dir) = connect().await;
    serve(
        &server,
        "GET",
        "/apis/apps/v1/namespaces/demo/deployments",
        json(
            200,
            &quench_serde::json!({"items": [
                workload_object("web", "demo", Some("shop"), &quench_serde::json!({"spec": {"replicas": 3}, "status": {"readyReplicas": 2}})),
                // not a package's: no label
                workload_object("stray", "demo", None, &quench_serde::json!({})),
                // replicas default to 1 when unset, ready to 0
                workload_object("api", "demo", Some("shop"), &quench_serde::json!({})),
            ]}),
        ),
    )
    .await;
    serve(
        &server,
        "GET",
        "/apis/apps/v1/namespaces/demo/statefulsets",
        json(200, &quench_serde::json!({"items": [
            workload_object("db", "demo", Some("shop"), &quench_serde::json!({"spec": {"replicas": 1}, "status": {"readyReplicas": 1}})),
        ]})),
    )
    .await;
    serve(
        &server,
        "GET",
        "/apis/apps/v1/namespaces/demo/daemonsets",
        json(200, &quench_serde::json!({"items": [
            workload_object("agent", "demo", Some("shop"), &quench_serde::json!({"status": {"desiredNumberScheduled": 5, "numberReady": 4}})),
        ]})),
    )
    .await;

    let found = cluster.list_workloads(&["demo".to_string()]).await.unwrap();
    let summary: Vec<(&str, &str, i32, i32)> = found
        .iter()
        .map(|w: &Workload| (w.kind.as_str(), w.name.as_str(), w.desired, w.ready))
        .collect();
    // sorted by namespace, kind, then name
    assert_eq!(
        summary,
        [
            ("daemonset", "agent", 5, 4),
            ("deployment", "api", 1, 0),
            ("deployment", "web", 3, 2),
            ("statefulset", "db", 1, 1),
        ]
    );
    assert_eq!(found[2].package, "shop");
    assert_eq!(found[2].version.as_deref(), Some("1.2.0+b3"));

    // everywhere: the same kinds, asked for without a namespace
    server.reset().await;
    for kind in ["deployments", "statefulsets", "daemonsets"] {
        serve(
            &server,
            "GET",
            &format!("/apis/apps/v1/{kind}"),
            json(200, &quench_serde::json!({"items": []})),
        )
        .await;
    }
    assert!(cluster.list_workloads(&[]).await.unwrap().is_empty());

    // a kind the account may not list fails the call
    server.reset().await;
    serve(
        &server,
        "GET",
        "/apis/apps/v1/namespaces/demo/deployments",
        ResponseTemplate::new(403),
    )
    .await;
    assert!(cluster.list_workloads(&["demo".to_string()]).await.is_err());
}

/// The discovery documents of a small cluster, and its objects.
#[allow(clippy::too_many_lines)]
async fn serve_cluster(server: &MockServer) {
    serve(
        server,
        "GET",
        "/apis",
        json(
            200,
            &quench_serde::json!({"groups": [
                {"name": "apps", "preferredVersion": {"version": "v1"}},
                {"name": "networking.k8s.io", "preferredVersion": {"version": "v1"}},
                {"name": "unreadable.example", "preferredVersion": {"version": "v1"}},
                {"name": "broken.example"},
            ]}),
        ),
    )
    .await;
    serve(
        server,
        "GET",
        "/api/v1",
        json(
            200,
            &quench_serde::json!({"resources": [
                {"name": "configmaps", "kind": "ConfigMap", "namespaced": true, "verbs": ["get", "list"]},
                {"name": "secrets", "kind": "Secret", "namespaced": true, "verbs": ["get", "list"]},
                {"name": "pods", "kind": "Pod", "namespaced": true, "verbs": ["get", "list"]},
                {"name": "pods/log", "kind": "Pod", "namespaced": true, "verbs": ["get"]},
                {"name": "namespaces", "kind": "Namespace", "namespaced": false, "verbs": ["get", "list"]},
                {"name": "bindings", "kind": "Binding", "namespaced": true, "verbs": ["create"]},
            ]}),
        ),
    )
    .await;
    serve(
        server,
        "GET",
        "/apis/apps/v1",
        json(
            200,
            &quench_serde::json!({"resources": [
                {"name": "deployments", "kind": "Deployment", "namespaced": true, "verbs": ["get", "list"]},
                {"name": "replicasets", "kind": "ReplicaSet", "namespaced": true, "verbs": ["get", "list"]},
            ]}),
        ),
    )
    .await;
    serve(
        server,
        "GET",
        "/apis/networking.k8s.io/v1",
        json(
            200,
            &quench_serde::json!({"resources": [
                {"name": "ingresses", "kind": "Ingress", "namespaced": true, "verbs": ["get", "list"]},
            ]}),
        ),
    )
    .await;
    serve(
        server,
        "GET",
        "/apis/unreadable.example/v1",
        ResponseTemplate::new(403),
    )
    .await;

    let item = |kind: &str, name: &str, ns: Option<&str>, package: &str, owned: bool| {
        let mut metadata = quench_serde::json!({
            "name": name,
            "labels": {"riveter.forge/package": package},
            "annotations": {"riveter.forge/package-version": "2.0.0", "gantry.forge/edited": "true"},
        });
        if let Some(ns) = ns {
            metadata["namespace"] = quench_serde::json!(ns);
        }
        if owned {
            metadata["ownerReferences"] = quench_serde::json!([{"kind": "Ingress", "name": "x"}]);
        }
        let _ = kind;
        quench_serde::json!({"metadata": metadata})
    };
    let mut web = item("Deployment", "web", Some("demo"), "shop", false);
    web["spec"] = quench_serde::json!({"replicas": 3});
    web["status"] = quench_serde::json!({"readyReplicas": 3});
    serve(
        server,
        "GET",
        "/apis/apps/v1/deployments",
        json(200, &quench_serde::json!({"items": [web]})),
    )
    .await;
    serve(
        server,
        "GET",
        "/apis/networking.k8s.io/v1/ingresses",
        json(
            200,
            &quench_serde::json!({"items": [
                item("Ingress", "web", Some("demo"), "shop", false),
                item("Ingress", "elsewhere", Some("other"), "shop", false),
            ]}),
        ),
    )
    .await;
    serve(
        server,
        "GET",
        "/api/v1/configmaps",
        json(200, &quench_serde::json!({"items": [item("ConfigMap", "settings", Some("demo"), "shop", true)]})),
    )
    .await;
    serve(
        server,
        "GET",
        "/api/v1/namespaces",
        json(
            200,
            &quench_serde::json!({"items": [item("Namespace", "demo", None, "shop", false)]}),
        ),
    )
    .await;
}

#[allow(clippy::await_holding_lock)]
#[quench_test::test]
async fn every_resource_a_package_installed_is_found_through_discovery() {
    let _env = env_lock();
    let (server, cluster, _dir) = connect().await;
    serve_cluster(&server).await;

    let all = cluster.list_resources(&[]).await.unwrap();
    let names: Vec<(&str, &str)> = all
        .iter()
        .map(|r: &LiveResource| (r.kind.as_str(), r.name.as_str()))
        .collect();
    assert_eq!(
        names,
        [
            ("ConfigMap", "settings"),
            ("Deployment", "web"),
            ("Ingress", "elsewhere"),
            ("Ingress", "web"),
            ("Namespace", "demo"),
        ],
        "pods, secrets, replica sets and verbs that cannot list are never shown"
    );
    let deployment = all.iter().find(|r| r.kind == "Deployment").unwrap();
    assert_eq!(deployment.ready.as_deref(), Some("3/3"));
    assert_eq!(deployment.api_version.as_deref(), Some("apps/v1"));
    assert_eq!(deployment.version.as_deref(), Some("2.0.0"));
    assert!(deployment.edited && !deployment.owned);
    assert!(all.iter().find(|r| r.kind == "ConfigMap").unwrap().owned);
    assert!(
        all.iter()
            .find(|r| r.kind == "Namespace")
            .unwrap()
            .namespace
            .is_none()
    );

    // an allow-list keeps its namespaces, and cluster-scoped kinds stay visible
    let allowed = cluster.list_resources(&["demo".to_string()]).await.unwrap();
    let kinds: Vec<&str> = allowed.iter().map(|r| r.kind.as_str()).collect();
    assert!(!allowed.iter().any(|r| r.name == "elsewhere"), "{kinds:?}");
    assert!(kinds.contains(&"Namespace"), "{kinds:?}");

    // discovery is kept for a minute, so a second call does not repeat it
    let discoveries = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.path == "/apis")
        .count();
    assert_eq!(discoveries, 1);
}

#[allow(clippy::await_holding_lock)]
#[quench_test::test]
async fn a_kind_the_account_may_not_list_is_skipped_not_fatal() {
    let _env = env_lock();
    let (server, cluster, _dir) = connect().await;
    serve_cluster(&server).await;
    server.reset().await;
    serve_cluster(&server).await;
    // replace the ingress list with a refusal: it is simply not shown
    Mock::given(method("GET"))
        .and(path("/apis/networking.k8s.io/v1/ingresses"))
        .respond_with(ResponseTemplate::new(403))
        .with_priority(1)
        .mount(&server)
        .await;
    let all = cluster.list_resources(&[]).await.unwrap();
    assert!(!all.iter().any(|r| r.kind == "Ingress"));
    assert!(all.iter().any(|r| r.kind == "Deployment"));
}

#[allow(clippy::await_holding_lock)]
#[quench_test::test]
async fn one_resource_is_shown_as_yaml_without_what_the_server_added() {
    let _env = env_lock();
    let (server, cluster, _dir) = connect().await;
    serve_cluster(&server).await;
    serve(
        &server,
        "GET",
        "/apis/apps/v1/namespaces/demo/deployments/web",
        json(
            200,
            &quench_serde::json!({
                "apiVersion": "apps/v1",
                "kind": "Deployment",
                "metadata": {
                    "name": "web", "namespace": "demo", "uid": "abc", "resourceVersion": "9",
                    "managedFields": [{}], "generation": 4,
                    "annotations": {
                        "kubectl.kubernetes.io/last-applied-configuration": "{}",
                        "deployment.kubernetes.io/revision": "3",
                        "keep": "me",
                    },
                },
                "spec": {"replicas": 3},
                "status": {"readyReplicas": 3},
            }),
        ),
    )
    .await;
    serve(
        &server,
        "GET",
        "/api/v1/namespaces/demo",
        json(
            200,
            &quench_serde::json!({"kind": "Namespace", "metadata": {"name": "demo"}}),
        ),
    )
    .await;

    let yaml = cluster
        .resource_yaml(Some("apps/v1"), "deployment", Some("demo"), "web")
        .await
        .unwrap();
    assert!(
        yaml.contains("kind: Deployment") && yaml.contains("replicas: 3"),
        "{yaml}"
    );
    assert!(yaml.contains("keep: me"), "{yaml}");
    for removed in [
        "status",
        "managedFields",
        "resourceVersion",
        "uid",
        "last-applied",
        "revision",
    ] {
        assert!(!yaml.contains(removed), "{removed} in {yaml}");
    }

    // a cluster-scoped kind is read without a namespace
    let namespace = cluster
        .resource_yaml(None, "Namespace", None, "demo")
        .await
        .unwrap();
    assert!(namespace.contains("name: demo"), "{namespace}");

    // a Secret is never shown; an unknown kind or version is named
    let secret = cluster
        .resource_yaml(None, "Secret", Some("demo"), "creds")
        .await
        .unwrap_err();
    assert!(secret.to_string().contains("never shown"), "{secret}");
    let unknown = cluster
        .resource_yaml(None, "Gadget", Some("demo"), "x")
        .await
        .unwrap_err();
    assert!(unknown.to_string().contains("no kind Gadget"), "{unknown}");
    let wrong_version = cluster
        .resource_yaml(Some("apps/v9"), "Deployment", Some("demo"), "web")
        .await;
    assert!(wrong_version.is_err());
    // a missing object is the server's answer, passed on
    assert!(
        cluster
            .resource_yaml(None, "Deployment", Some("demo"), "nope")
            .await
            .is_err()
    );
}

#[allow(clippy::await_holding_lock)]
#[quench_test::test]
async fn connecting_needs_a_readable_kubeconfig() {
    let _env = env_lock();
    let dir = quench_core::tmp::TempDir::new().unwrap();
    quench_core::env::set(
        "KUBECONFIG",
        dir.path().join("missing").display().to_string(),
    );
    let mut settings = Settings::inert();
    settings.cluster = ClusterMode::Kubeconfig;
    // no such file, no `~/.kube/config` to fall back to, and not in a cluster
    let home = std::env::var("HOME").ok();
    quench_core::env::set("HOME", dir.path().display().to_string());
    quench_core::env::remove("KUBERNETES_SERVICE_HOST");
    let result = KubeCluster::connect(&settings);
    if let Some(home) = home {
        quench_core::env::set("HOME", home);
    }
    let Err(error) = result else {
        panic!("a cluster without a kubeconfig")
    };
    assert!(error.to_string().starts_with("kubernetes:"), "{error}");
}
