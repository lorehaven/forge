//! Talks to a real cluster; ignored unless asked for. Run it with a scratch namespace that holds a
//! `gantry-runner` service account and a `gantry-runner-credentials` Secret:
//!
//! ```text
//! kubectl create namespace gantry-it
//! kubectl -n gantry-it create serviceaccount gantry-runner
//! kubectl -n gantry-it create secret generic gantry-runner-credentials
//! GANTRY_IT_NAMESPACE=gantry-it GANTRY_IT_IMAGE=rancher/mirrored-pause:3.6 \
//!     cargo test -p gantry-service --test cluster_it -- --ignored
//! ```
//!
//! The image has no `gantry-runner` in it, so the Job's pod cannot start - which is the point: the API
//! server has to accept the manifest, and the Job has to be seen going from active to failed.

use gantry_service::domain::cluster::{Cluster, JobSpec, JobStatus, KubeCluster, PACKAGE_LABEL};
use gantry_service::domain::settings::{ClusterMode, Settings};
use std::time::Duration;

fn settings() -> Settings {
    let mut settings = Settings::inert();
    settings.cluster = ClusterMode::Kubeconfig;
    settings
}

#[tokio::test]
#[ignore = "needs a cluster; see the module docs"]
async fn a_runner_job_is_accepted_seen_failing_read_and_deleted() {
    let namespace = std::env::var("GANTRY_IT_NAMESPACE").expect("GANTRY_IT_NAMESPACE");
    let image = std::env::var("GANTRY_IT_IMAGE").expect("GANTRY_IT_IMAGE");
    let cluster = KubeCluster::connect(&settings())
        .await
        .expect("a kubeconfig");

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

#[tokio::test]
#[ignore = "needs a cluster; see the module docs"]
async fn workloads_a_package_installed_are_found_by_their_label_and_version() {
    let namespace = std::env::var("GANTRY_IT_NAMESPACE").expect("GANTRY_IT_NAMESPACE");
    let cluster = KubeCluster::connect(&settings())
        .await
        .expect("a kubeconfig");
    // Whatever is there is labelled by the package that put it there; nothing unlabelled appears.
    for workload in cluster.list_workloads(&[namespace]).await.unwrap() {
        assert!(
            !workload.package.is_empty(),
            "{PACKAGE_LABEL} is what selects a workload"
        );
    }
}
