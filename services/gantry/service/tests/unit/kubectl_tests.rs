//! The runner's `kubectl` commands, carried out against a fake API server: what each shape the planner writes does to the
//! cluster, and what it prints.

use crate::fake_cluster::{FakeCluster, with_cluster};
use gantry_service::domain::commands::Command;
use gantry_service::domain::runner::{Exec, ProcessExec};

fn args(line: &str) -> Vec<String> {
    line.split_whitespace().map(str::to_string).collect()
}

fn kubectl(line: &str) -> Command {
    Command {
        program: "kubectl".to_string(),
        args: args(line),
        mutating: true,
    }
}

const DEPLOYMENT: &str = r#"{"apiVersion":"apps/v1","kind":"Deployment","metadata":{"name":"web","namespace":"ml","uid":"u1","annotations":{"deployment.kubernetes.io/revision":"3"}},"spec":{"replicas":2,"template":{"metadata":{"labels":{"app":"web"}},"spec":{"containers":[{"name":"c","image":"web:3"}]}}}}"#;

fn replica_set(name: &str, revision: u32, image: &str) -> String {
    format!(
        r#"{{"apiVersion":"apps/v1","kind":"ReplicaSet","metadata":{{"name":"{name}","namespace":"ml","annotations":{{"deployment.kubernetes.io/revision":"{revision}"}},"ownerReferences":[{{"kind":"Deployment","name":"web","uid":"u1"}}]}},"spec":{{"template":{{"metadata":{{"labels":{{"app":"web","pod-template-hash":"h{revision}"}}}},"spec":{{"containers":[{{"name":"c","image":"{image}"}}]}}}}}}}}"#
    )
}

fn pod(name: &str, app: &str) -> String {
    format!(
        r#"{{"apiVersion":"v1","kind":"Pod","metadata":{{"name":"{name}","namespace":"ml","labels":{{"app":"{app}"}}}}}}"#
    )
}

#[quench_test::test]
fn scale_sets_the_replica_count() {
    let cluster = FakeCluster::start();
    cluster.insert("deployments", DEPLOYMENT);
    with_cluster(&cluster, || {
        let code = ProcessExec
            .run(&kubectl("-n ml scale deployment/web --replicas=5"))
            .unwrap();
        assert_eq!(code, 0);
    });
    assert_eq!(
        cluster.object("deployments", "ml", "web").unwrap()["spec"]["replicas"],
        5
    );
}

#[quench_test::test]
fn a_command_that_fails_exits_nonzero() {
    let cluster = FakeCluster::start();
    with_cluster(&cluster, || {
        let code = ProcessExec
            .run(&kubectl("-n ml scale deployment/gone --replicas=1"))
            .unwrap();
        assert_eq!(code, 1);
        assert_eq!(ProcessExec.run(&kubectl("frobnicate")).unwrap(), 1);
    });
}

#[quench_test::test]
fn apply_sends_the_objects_of_a_file() {
    let cluster = FakeCluster::start();
    let dir = quench_core::tmp::TempDir::new().unwrap();
    let file = dir.path().join("edit.yaml");
    std::fs::write(
        &file,
        "apiVersion: v1\nkind: ConfigMap\nmetadata: {name: c, namespace: ml}\ndata: {a: b}\n",
    )
    .unwrap();
    with_cluster(&cluster, || {
        let code = ProcessExec
            .run(&kubectl(&format!("apply -f {}", file.display())))
            .unwrap();
        assert_eq!(code, 0);
    });
    assert!(cluster.has("configmaps", "ml", "c"));
}

#[quench_test::test]
fn delete_waits_for_the_object_and_may_ignore_a_missing_one() {
    let cluster = FakeCluster::start();
    cluster.insert("deployments", DEPLOYMENT);
    with_cluster(&cluster, || {
        let mut exec = ProcessExec;
        assert_eq!(
            exec.run(&kubectl(
                "delete deployment.apps/web --ignore-not-found --timeout=20s -n ml"
            ))
            .unwrap(),
            0
        );
        assert!(!cluster.has("deployments", "ml", "web"));
        // gone already: fine with --ignore-not-found, an error without
        assert_eq!(
            exec.run(&kubectl(
                "delete deployment.apps/web --ignore-not-found --timeout=20s -n ml"
            ))
            .unwrap(),
            0
        );
        assert_eq!(
            exec.run(&kubectl("delete deployment.apps/web --timeout=20s -n ml"))
                .unwrap(),
            1
        );
    });
}

#[quench_test::test]
fn pods_are_deleted_and_listed_by_selector_in_a_namespace_or_in_all() {
    let cluster = FakeCluster::start();
    cluster.insert("pods", &pod("a", "web"));
    cluster.insert("pods", &pod("b", "web"));
    cluster.insert("pods", &pod("c", "other"));
    with_cluster(&cluster, || {
        let mut exec = ProcessExec;
        let (code, names) = exec
            .capture(&kubectl("get pods -l app=web -o name -A"))
            .unwrap();
        assert_eq!(code, 0);
        let mut names: Vec<&str> = names.lines().collect();
        names.sort_unstable();
        assert_eq!(names, ["pod/a", "pod/b"]);

        assert_eq!(
            exec.run(&kubectl("delete pods -l app=web --wait=false -n ml"))
                .unwrap(),
            0
        );
        let (_, names) = exec
            .capture(&kubectl("get pods -l app=web -o name -n ml"))
            .unwrap();
        assert_eq!(names, "");
    });
    assert!(
        cluster.has("pods", "ml", "c"),
        "what the selector did not match stays"
    );
}

#[quench_test::test]
fn workloads_are_listed_as_json_with_their_kind() {
    let cluster = FakeCluster::start();
    cluster.insert("deployments", DEPLOYMENT);
    with_cluster(&cluster, || {
        let (code, json) = ProcessExec
            .capture(&kubectl(
                "-n ml get deployments,statefulsets,daemonsets -o json",
            ))
            .unwrap();
        assert_eq!(code, 0);
        let document: quench_serde::Value = quench_serde::json::from_str(&json).unwrap();
        assert_eq!(document["items"][0]["kind"], "Deployment");
        assert_eq!(document["items"][0]["metadata"]["name"], "web");
    });
}

#[quench_test::test]
fn rollout_status_returns_when_the_workload_is_rolled_out() {
    let cluster = FakeCluster::start();
    with_cluster(&cluster, || {
        // applied through the fake, which rolls a Deployment out at once
        let dir = quench_core::tmp::TempDir::new().unwrap();
        let file = dir.path().join("d.yaml");
        std::fs::write(&file, "apiVersion: apps/v1\nkind: Deployment\nmetadata: {name: web, namespace: ml}\nspec: {replicas: 1}\n").unwrap();
        let mut exec = ProcessExec;
        assert_eq!(
            exec.run(&kubectl(&format!("apply -f {}", file.display())))
                .unwrap(),
            0
        );
        assert_eq!(
            exec.run(&kubectl(
                "-n ml rollout status deployment/web --timeout=20s"
            ))
            .unwrap(),
            0
        );
        assert_eq!(
            exec.run(&kubectl(
                "-n ml rollout status deployment/nope --timeout=2s"
            ))
            .unwrap(),
            1
        );
    });
}

#[quench_test::test]
fn restart_changes_the_pod_template() {
    let cluster = FakeCluster::start();
    cluster.insert("deployments", DEPLOYMENT);
    with_cluster(&cluster, || {
        assert_eq!(
            ProcessExec
                .run(&kubectl("-n ml rollout restart deployment/web"))
                .unwrap(),
            0
        );
    });
    let object = cluster.object("deployments", "ml", "web").unwrap();
    assert!(
        object["spec"]["template"]["metadata"]["annotations"]["kubectl.kubernetes.io/restartedAt"]
            .as_str()
            .is_some(),
        "{object}"
    );
}

#[quench_test::test]
fn undo_puts_back_the_previous_revisions_template_without_its_hash() {
    let cluster = FakeCluster::start();
    cluster.insert("deployments", DEPLOYMENT);
    cluster.insert("replicasets", &replica_set("web-h1", 1, "web:1"));
    cluster.insert("replicasets", &replica_set("web-h2", 2, "web:2"));
    cluster.insert("replicasets", &replica_set("web-h3", 3, "web:3"));
    with_cluster(&cluster, || {
        assert_eq!(
            ProcessExec
                .run(&kubectl("-n ml rollout undo deployment/web"))
                .unwrap(),
            0
        );
    });
    let object = cluster.object("deployments", "ml", "web").unwrap();
    assert_eq!(
        object["spec"]["template"]["spec"]["containers"][0]["image"],
        "web:2"
    );
    assert_eq!(
        object["spec"]["template"]["metadata"]["labels"]["pod-template-hash"],
        quench_serde::Value::Null
    );
}

#[quench_test::test]
fn undo_with_nothing_earlier_fails() {
    let cluster = FakeCluster::start();
    cluster.insert("deployments", DEPLOYMENT);
    cluster.insert("replicasets", &replica_set("web-h3", 3, "web:3"));
    with_cluster(&cluster, || {
        assert_eq!(
            ProcessExec
                .run(&kubectl("-n ml rollout undo deployment/web"))
                .unwrap(),
            1
        );
    });
}
