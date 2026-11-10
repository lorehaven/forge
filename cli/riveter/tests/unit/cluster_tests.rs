//! Riveter talking to a cluster directly: reading manifests, applying, diffing, deleting, listing and waiting, against a
//! fake API server, and the judgement of a rollout and of a Job from the object alone.

use crate::fake_cluster::{FakeCluster, JobMode, with_cluster};
use quench_serde::{Value, json, yaml};
use riveter::cluster::{
    Cluster, JobState, Progress, documents, dry_run, job_state_of, label, rollout_progress,
};
use std::time::Duration;

fn docs(text: &str) -> Vec<Value> {
    documents(text).unwrap()
}

const WEB: &str = "\
apiVersion: apps/v1
kind: Deployment
metadata: {name: web, namespace: demo, labels: {app.kubernetes.io/managed-by: riveter, env: dev}}
spec: {replicas: 2, selector: {matchLabels: {app: web}}}
---
apiVersion: v1
kind: Service
metadata: {name: web, namespace: demo}
spec: {selector: {app: web}}
";

// --- reading manifests ----------------------------------------------------------

#[quench_test::test]
fn a_manifest_is_its_objects_in_order_without_empty_documents() {
    let found = docs(
        "---\n# nothing here\n---\napiVersion: v1\nkind: Namespace\nmetadata: {name: a}\n---\n---\napiVersion: v1\nkind: Namespace\nmetadata: {name: b}\n",
    );
    let names: Vec<&str> = found
        .iter()
        .map(|d| d["metadata"]["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["a", "b"]);
}

#[quench_test::test]
fn a_list_is_opened_up() {
    let found = docs(
        "apiVersion: v1\nkind: List\nitems:\n  - {apiVersion: v1, kind: Namespace, metadata: {name: a}}\n  - {apiVersion: v1, kind: Namespace, metadata: {name: b}}\n",
    );
    assert_eq!(found.len(), 2);
    assert_eq!(found[1]["metadata"]["name"], "b");
}

#[quench_test::test]
fn objects_are_named_the_way_kubectl_names_them() {
    let found = docs(WEB);
    assert_eq!(label(&found[0]), "deployment.apps/web");
    assert_eq!(label(&found[1]), "service/web");
}

#[quench_test::test]
fn a_dry_run_names_what_it_would_send_and_asks_nothing() {
    assert_eq!(dry_run(&docs(WEB)).unwrap(), 2);
}

// --- applying ---------------------------------------------------------------------

#[quench_test::test]
fn applying_creates_and_then_leaves_unchanged_what_is_the_same() {
    let cluster = FakeCluster::start();
    with_cluster(&cluster, || {
        let connection = Cluster::connect(None).unwrap();
        assert_eq!(connection.apply(&docs(WEB)).unwrap(), 2);
        assert!(cluster.has("deployments", "demo", "web"));
        assert!(cluster.has("services", "demo", "web"));
        let first = cluster.object("deployments", "demo", "web").unwrap();

        // The same again changes nothing, so the version does not move.
        connection.apply(&docs(WEB)).unwrap();
        let second = cluster.object("deployments", "demo", "web").unwrap();
        assert_eq!(
            first["metadata"]["resourceVersion"],
            second["metadata"]["resourceVersion"]
        );

        // A change does.
        connection
            .apply(&docs(&WEB.replace("replicas: 2", "replicas: 3")))
            .unwrap();
        let third = cluster.object("deployments", "demo", "web").unwrap();
        assert_ne!(
            second["metadata"]["resourceVersion"],
            third["metadata"]["resourceVersion"]
        );
        assert_eq!(third["spec"]["replicas"], 3);
    });
}

#[quench_test::test]
fn the_namespace_of_a_context_is_used_when_a_manifest_names_none() {
    let cluster = FakeCluster::start();
    with_cluster(&cluster, || {
        let connection = Cluster::connect(None).unwrap();
        connection
            .apply(&docs(
                "apiVersion: v1\nkind: ConfigMap\nmetadata: {name: c}\ndata: {a: b}\n",
            ))
            .unwrap();
        // the fake kubeconfig's context says `default`
        assert!(cluster.has("configmaps", "default", "c"));
    });
}

#[quench_test::test]
fn one_refused_object_does_not_stop_the_others_but_fails_the_apply() {
    let cluster = FakeCluster::start();
    cluster.refuses("deployment/web");
    with_cluster(&cluster, || {
        let connection = Cluster::connect(None).unwrap();
        let err = connection.apply(&docs(WEB)).unwrap_err().to_string();
        assert!(err.contains("1 of 2"), "{err}");
        assert!(err.contains("deployment.apps/web"), "{err}");
        assert!(
            cluster.has("services", "demo", "web"),
            "the service still went out"
        );
    });
}

#[quench_test::test]
fn a_kind_the_cluster_does_not_serve_is_said_plainly() {
    let cluster = FakeCluster::start();
    with_cluster(&cluster, || {
        let connection = Cluster::connect(None).unwrap();
        let err = connection
            .apply(&docs("apiVersion: cert-manager.io/v1\nkind: Certificate\nmetadata: {name: c, namespace: demo}\n"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("certificate.cert-manager.io/c"), "{err}");
    });
}

// --- diff ----------------------------------------------------------------------------

#[quench_test::test]
fn a_diff_is_empty_when_the_cluster_matches_and_shows_lines_when_it_does_not() {
    let cluster = FakeCluster::start();
    with_cluster(&cluster, || {
        let connection = Cluster::connect(None).unwrap();
        assert!(connection.diff(&docs(WEB)).unwrap(), "nothing is there yet");
        assert!(
            !cluster.has("deployments", "demo", "web"),
            "a diff writes nothing"
        );

        connection.apply(&docs(WEB)).unwrap();
        assert!(!connection.diff(&docs(WEB)).unwrap());
        assert!(
            connection
                .diff(&docs(&WEB.replace("replicas: 2", "replicas: 5")))
                .unwrap()
        );
    });
}

// --- delete ----------------------------------------------------------------------------

#[quench_test::test]
fn deleting_removes_the_objects_and_a_missing_one_is_an_error_after_the_rest() {
    let cluster = FakeCluster::start();
    with_cluster(&cluster, || {
        let connection = Cluster::connect(None).unwrap();
        connection.apply(&docs(WEB)).unwrap();
        assert_eq!(connection.delete(&docs(WEB)).unwrap(), 2);
        assert!(!cluster.has("deployments", "demo", "web"));
        assert!(!cluster.has("services", "demo", "web"));

        let err = connection.delete(&docs(WEB)).unwrap_err().to_string();
        assert!(err.contains("deployment.apps/web"), "{err}");
    });
}

#[quench_test::test]
fn delete_named_takes_a_lowercase_kind() {
    let cluster = FakeCluster::start();
    with_cluster(&cluster, || {
        let connection = Cluster::connect(None).unwrap();
        connection.apply(&docs(WEB)).unwrap();
        connection
            .delete_named("deployment", "web", Some("demo"))
            .unwrap();
        assert!(!cluster.has("deployments", "demo", "web"));
        assert!(
            connection
                .delete_named("nonsense", "x", Some("demo"))
                .is_err()
        );
    });
}

// --- listing and CRDs -----------------------------------------------------------------------

#[quench_test::test]
fn listing_by_label_finds_only_what_carries_it() {
    let cluster = FakeCluster::start();
    with_cluster(&cluster, || {
        let connection = Cluster::connect(None).unwrap();
        connection.apply(&docs(WEB)).unwrap();
        let found = connection
            .list_labelled(
                &["deployment", "service", "ingress"],
                "app.kubernetes.io/managed-by=riveter,env=dev",
                Some("demo"),
            )
            .unwrap();
        // only the deployment is labelled; `ingress` is a kind this cluster does not serve and is left out
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].kind, "Deployment");
        assert_eq!(found[0].name, "web");
        assert_eq!(found[0].owner_kinds, [] as [String; 0]);
    });
}

#[quench_test::test]
fn crds_are_fetched_by_name_or_all_and_a_missing_one_is_left_out() {
    let cluster = FakeCluster::start();
    for name in ["a.example.io", "b.example.io"] {
        cluster.insert(
            "customresourcedefinitions",
            &format!(r#"{{"apiVersion":"apiextensions.k8s.io/v1","kind":"CustomResourceDefinition","metadata":{{"name":"{name}"}}}}"#),
        );
    }
    with_cluster(&cluster, || {
        let connection = Cluster::connect(None).unwrap();
        let some = connection
            .crds(&["a.example.io".to_string(), "gone.example.io".to_string()])
            .unwrap();
        assert_eq!(some.len(), 1);
        assert_eq!(connection.crds(&[]).unwrap().len(), 2);
    });
}

// --- waiting ------------------------------------------------------------------------------------

#[quench_test::test]
fn a_rollout_that_is_done_returns_and_a_missing_workload_is_an_error() {
    let cluster = FakeCluster::start();
    with_cluster(&cluster, || {
        let connection = Cluster::connect(None).unwrap();
        connection.apply(&docs(WEB)).unwrap();
        connection
            .rollout_status("deployment", "web", Some("demo"), Duration::from_secs(5))
            .unwrap();
        assert!(
            connection
                .rollout_status("deployment", "nope", Some("demo"), Duration::from_secs(1))
                .is_err()
        );
        assert!(
            connection
                .rollout_status("service", "web", Some("demo"), Duration::from_secs(1))
                .is_err()
        );
    });
}

#[quench_test::test]
fn a_job_is_as_the_cluster_says() {
    let cluster = FakeCluster::start();
    with_cluster(&cluster, || {
        let connection = Cluster::connect(None).unwrap();
        connection
            .apply(&docs(
                "apiVersion: batch/v1\nkind: Job\nmetadata: {name: j, namespace: demo}\nspec: {}\n",
            ))
            .unwrap();
        for (mode, expected) in [
            (JobMode::Running, JobState::Running),
            (JobMode::Complete, JobState::Complete),
            (
                JobMode::Failed("boom".into()),
                JobState::Failed("boom".into()),
            ),
        ] {
            cluster.job_is(mode);
            assert_eq!(connection.job_state("j", Some("demo")).unwrap(), expected);
        }
    });
}

// --- judging a rollout from the object ---------------------------------------------------------------

fn object(text: &str) -> Value {
    yaml::from_str(text).unwrap()
}

#[quench_test::test]
fn a_deployment_is_judged_as_kubectl_does() {
    let waiting = |text: &str| match rollout_progress("Deployment", &object(text)) {
        Progress::Waiting(message) => message,
        other => panic!("{other:?}"),
    };
    let base = "metadata: {name: web, generation: 2}\n";

    assert_eq!(
        waiting(&format!("{base}status: {{observedGeneration: 1}}")),
        "Waiting for deployment spec update to be observed..."
    );
    assert_eq!(
        waiting(&format!(
            "{base}spec: {{replicas: 3}}\nstatus: {{observedGeneration: 2, updatedReplicas: 1, replicas: 3}}"
        )),
        "Waiting for deployment \"web\" rollout to finish: 1 out of 3 new replicas have been updated..."
    );
    assert_eq!(
        waiting(&format!(
            "{base}spec: {{replicas: 2}}\nstatus: {{observedGeneration: 2, updatedReplicas: 2, replicas: 3, availableReplicas: 2}}"
        )),
        "Waiting for deployment \"web\" rollout to finish: 1 old replicas are pending termination..."
    );
    assert_eq!(
        waiting(&format!(
            "{base}spec: {{replicas: 2}}\nstatus: {{observedGeneration: 2, updatedReplicas: 2, replicas: 2, availableReplicas: 1}}"
        )),
        "Waiting for deployment \"web\" rollout to finish: 1 of 2 updated replicas are available..."
    );
    assert_eq!(
        rollout_progress(
            "Deployment",
            &object(&format!(
                "{base}spec: {{replicas: 2}}\nstatus: {{observedGeneration: 2, updatedReplicas: 2, replicas: 2, availableReplicas: 2}}"
            ))
        ),
        Progress::Done("deployment \"web\" successfully rolled out".to_string())
    );
    assert!(matches!(
        rollout_progress(
            "Deployment",
            &object(&format!("{base}status: {{observedGeneration: 2, conditions: [{{type: Progressing, reason: ProgressDeadlineExceeded}}]}}"))
        ),
        Progress::Failed(why) if why.contains("exceeded its progress deadline")
    ));
}

#[quench_test::test]
fn a_statefulset_and_a_daemonset_are_judged_by_their_own_counts() {
    let stateful = |text: &str| rollout_progress("StatefulSet", &object(text));
    assert!(matches!(
        stateful("metadata: {name: db, generation: 1}\nspec: {replicas: 3}\nstatus: {observedGeneration: 1, readyReplicas: 1}"),
        Progress::Waiting(m) if m == "Waiting for 2 pods to be ready..."
    ));
    assert!(matches!(
        stateful(
            "metadata: {name: db, generation: 1}\nspec: {replicas: 3}\nstatus: {observedGeneration: 1, readyReplicas: 3, updatedReplicas: 3, currentRevision: r1, updateRevision: r1}"
        ),
        Progress::Done(_)
    ));
    assert!(matches!(
        stateful(
            "metadata: {name: db, generation: 1}\nspec: {replicas: 3, updateStrategy: {type: OnDelete}}\nstatus: {observedGeneration: 1}"
        ),
        Progress::Failed(_)
    ));

    let daemon = |text: &str| rollout_progress("DaemonSet", &object(text));
    assert!(matches!(
        daemon("metadata: {name: d, generation: 1}\nstatus: {observedGeneration: 1, desiredNumberScheduled: 3, updatedNumberScheduled: 2}"),
        Progress::Waiting(m) if m.contains("2 out of 3 new pods")
    ));
    assert!(matches!(
        daemon(
            "metadata: {name: d, generation: 1}\nstatus: {observedGeneration: 1, desiredNumberScheduled: 3, updatedNumberScheduled: 3, numberAvailable: 3}"
        ),
        Progress::Done(_)
    ));
}

#[quench_test::test]
fn a_job_is_judged_by_its_conditions() {
    let job = |conditions: &str| {
        job_state_of(
            &json::from_str::<Value>(&format!("{{\"status\":{{\"conditions\":{conditions}}}}}"))
                .unwrap(),
        )
    };
    assert_eq!(job("[]"), JobState::Running);
    assert_eq!(
        job(r#"[{"type":"Complete","status":"True"}]"#),
        JobState::Complete
    );
    assert_eq!(
        job(r#"[{"type":"Complete","status":"False"}]"#),
        JobState::Running
    );
    assert_eq!(
        job(r#"[{"type":"Failed","status":"True","message":"backoff"}]"#),
        JobState::Failed("backoff".to_string())
    );
}
