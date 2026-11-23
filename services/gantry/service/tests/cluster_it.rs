//! Talks to a real cluster; ignored unless asked for. Run it with a scratch namespace that holds a
//! `gantry-runner` service account and a `gantry-runner-credentials` Secret:
//!
//! ```text
//! kubectl create namespace gantry-it
//! kubectl -n gantry-it create serviceaccount gantry-runner
//! kubectl -n gantry-it create secret generic gantry-runner-credentials
//! GANTRY_IT_NAMESPACE=gantry-it GANTRY_IT_IMAGE=rancher/mirrored-pause:3.6 \
//!     cargo test -p gantry --test cluster_it -- --ignored
//! ```
//!
//! The image has no `gantry-runner` in it, so the Job's pod cannot start - which is the point: the API
//! server has to accept the manifest, and the Job has to be seen going from active to failed.

// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports)]

use gantry_service::domain::cluster::{Cluster, JobSpec, JobStatus, KubeCluster, PACKAGE_LABEL};
use gantry_service::domain::settings::{ClusterMode, Settings};
use std::time::Duration;

fn settings() -> Settings {
    let mut settings = Settings::inert();
    settings.cluster = ClusterMode::Kubeconfig;
    settings
}

#[quench_test::test]
#[ignore = "needs a cluster; see the module docs"]
async fn a_runner_job_is_accepted_seen_failing_read_and_deleted() {
    let namespace = std::env::var("GANTRY_IT_NAMESPACE").expect("GANTRY_IT_NAMESPACE");
    let image = std::env::var("GANTRY_IT_IMAGE").expect("GANTRY_IT_IMAGE");
    let cluster = KubeCluster::connect(&settings()).expect("a kubeconfig");

    let spec = JobSpec {
        namespace: namespace.clone(),
        name: "gantry-op-it000000".into(),
        image,
        service_account: "gantry-runner".into(),
        plan_json: r#"{"steps":[]}"#.into(),
        allowed_namespaces: vec![namespace.clone()],
        credentials_secret: "gantry-runner-credentials".into(),
        age_key_secret: "gantry-age-key".into(),
        // Optional mounts: this Secret does not exist, and the Job must still be accepted.
        values_secrets: vec!["gantry-values-nothing".into()],
        operation_id: "it".into(),
        deadline_secs: 120,
    };

    assert_eq!(
        cluster.job_status(&namespace, &spec.name).await.unwrap(),
        JobStatus::Absent
    );
    cluster
        .create_job(&spec)
        .await
        .expect("the API server accepts the manifest");
    // Creating it again is not an error: a restarted service re-issues the same Job.
    cluster
        .create_job(&spec)
        .await
        .expect("creating twice is fine");

    let mut status = JobStatus::Active;
    for _ in 0..60 {
        status = cluster.job_status(&namespace, &spec.name).await.unwrap();
        if status != JobStatus::Active {
            break;
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    assert!(matches!(status, JobStatus::Failed(_)), "{status:?}");

    // The log of a pod that never started is empty, not an error.
    let log = cluster.job_log(&namespace, &spec.name).await.unwrap();
    assert!(log.len() < 10_000);

    cluster.delete_job(&namespace, &spec.name).await.unwrap();
    cluster
        .delete_job(&namespace, &spec.name)
        .await
        .expect("deleting what is gone is fine");
    for _ in 0..30 {
        if cluster.job_status(&namespace, &spec.name).await.unwrap() == JobStatus::Absent {
            return;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    panic!("the Job was not deleted");
}

#[quench_test::test]
#[ignore = "needs a cluster; see the module docs"]
async fn workloads_a_package_installed_are_found_by_their_label_and_version() {
    let namespace = std::env::var("GANTRY_IT_NAMESPACE").expect("GANTRY_IT_NAMESPACE");
    let cluster = KubeCluster::connect(&settings()).expect("a kubeconfig");
    // Whatever is there is labelled by the package that put it there; nothing unlabelled appears.
    for workload in cluster.list_workloads(&[namespace]).await.unwrap() {
        assert!(
            !workload.package.is_empty(),
            "{PACKAGE_LABEL} is what selects a workload"
        );
    }
}

quench_test::main!();

#[quench_test::test]
#[ignore = "needs a cluster; read-only"]
async fn the_cluster_can_be_browsed_by_kind_with_events_and_a_pod_log() {
    let cluster = KubeCluster::connect(&settings()).expect("a kubeconfig");

    let kinds = cluster.browse_kinds().await.unwrap();
    let names: Vec<&str> = kinds.iter().map(|k| k.kind.as_str()).collect();
    assert!(
        names.contains(&"Node") && names.contains(&"Pod"),
        "{names:?}"
    );
    assert!(!names.contains(&"Secret"));

    let nodes = cluster.browse_list("Node", None).await.unwrap();
    assert_ne!(nodes.len(), 0);
    assert!(
        nodes
            .iter()
            .all(|n| n.status == "Ready" || n.status == "NotReady")
    );

    let pods = cluster.browse_list("Pod", None).await.unwrap();
    let pod = pods
        .iter()
        .find(|p| p.status == "Running")
        .expect("a running pod");
    let namespace = pod.namespace.clone().expect("a namespace");
    let view = cluster
        .browse_object("Pod", Some(&namespace), &pod.name)
        .await
        .unwrap();
    assert!(view.yaml.contains("kind: Pod") && !view.yaml.contains("managedFields"));
    assert_ne!(view.containers.len(), 0);
    cluster
        .pod_log(
            &namespace,
            &pod.name,
            Some(&view.containers[view.containers.len() - 1]),
            5,
            false,
        )
        .await
        .unwrap();

    assert!(cluster.browse_list("Secret", None).await.is_err());

    let overview = cluster.browse_overview().await.unwrap();
    assert_ne!(overview.nodes_total, 0);
    assert!(overview.pods_total >= overview.pods_running);
    assert!(overview.cpu_total_m > 0);

    let facade = cluster.browse_facade().await.unwrap();
    assert_ne!(facade.len(), 0);
    assert!(facade.iter().any(|e| e.available), "{facade:?}");
    // One link per Ingress host, at a page: never an API, asset or registry path.
    for entry in &facade {
        assert!(
            !["/api", "/assets", "/v2", "/token", "(", "/share"]
                .iter()
                .any(|bad| entry.url.contains(bad)),
            "{}",
            entry.url
        );
    }
    eprintln!("{}", facade.iter().map(|e| format!("{} {}", e.available, e.url)).collect::<Vec<_>>().join("\n"));
}

/// Opens a real shell over the Kubernetes exec WebSocket: `GANTRY_IT_EXEC_POD=namespace/pod`.
#[quench_test::test]
#[ignore = "needs a cluster and a pod with /bin/sh; set GANTRY_IT_EXEC_POD=namespace/pod"]
async fn a_shell_in_a_real_pod_runs_a_command_and_answers() {
    use gantry_service::domain::exec::ExecEvent;

    let target = std::env::var("GANTRY_IT_EXEC_POD").expect("GANTRY_IT_EXEC_POD");
    let (namespace, pod) = target.split_once('/').expect("namespace/pod");
    let cluster = KubeCluster::connect(&settings()).expect("a kubeconfig");

    let mut shell = cluster.exec_open(namespace, pod, None).await.unwrap();
    shell
        .input
        .send(b"echo gantry-it-$((6*7))\n".to_vec())
        .await
        .unwrap();

    let mut seen = String::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    while !seen.contains("gantry-it-42\r\n") && tokio::time::Instant::now() < deadline {
        if let Ok(Some(ExecEvent::Output(bytes))) =
            tokio::time::timeout(Duration::from_secs(5), shell.output.recv()).await
        {
            seen.push_str(&String::from_utf8_lossy(&bytes));
        }
    }
    assert!(seen.contains("gantry-it-42"), "{seen:?}");
    shell.input.send(b"exit\n".to_vec()).await.unwrap();
}
