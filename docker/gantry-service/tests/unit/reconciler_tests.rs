use gantry_service::domain::cluster::{FakeCluster, JobStatus};
use gantry_service::domain::executor::{ClusterExecutor, Executor};
use gantry_service::domain::operation::{MemoryStore, State, Store, job_name};
use gantry_service::domain::reconciler::{Report, tick};
use gantry_service::domain::registry::MemoryRegistry;
use gantry_service::domain::service::{Gantry, SubmitError};
use gantry_service::domain::settings::Settings;
use gantry_service::domain::steps::{Plan, Step};
use std::sync::Arc;

struct Rig {
    store: Arc<MemoryStore>,
    cluster: Arc<FakeCluster>,
    settings: Settings,
}

impl Rig {
    fn new() -> Self {
        let mut settings = Settings::inert();
        settings.namespace = "forge".into();
        Self {
            store: Arc::new(MemoryStore::new()),
            cluster: Arc::new(FakeCluster::new()),
            settings,
        }
    }

    /// A Gantry over the shared store and cluster. Building a second one is a service restart: the
    /// in-process state is gone, only the database and the cluster remain.
    fn gantry(&self) -> Gantry {
        let executor: Arc<dyn Executor> = Arc::new(ClusterExecutor {
            cluster: self.cluster.clone(),
            settings: self.settings.clone(),
        });
        Gantry::new(
            self.store.clone(),
            self.cluster.clone(),
            executor,
            Arc::new(MemoryRegistry::default()),
            self.settings.clone(),
        )
    }
}

fn plan() -> Plan {
    Plan {
        steps: vec![Step::Install {
            package: "media".into(),
            version: "1.0.0".into(),
            namespace: "media".into(),
            sets: Default::default(),
            replicas: Default::default(),
            values_secret: Some("gantry-values-media".into()),
            timeout_secs: None,
            targets: vec![],
            except: vec![],
            no_wait: false,
        }],
        summary: vec![],
        ..Default::default()
    }
}

#[tokio::test]
async fn a_submitted_operation_becomes_a_runner_job() {
    let rig = Rig::new();
    let gantry = rig.gantry();
    let operation = gantry
        .submit("install", "Install media", plan(), "ana")
        .await
        .unwrap();
    assert_eq!(operation.state, State::Queued);

    let report = tick(&gantry).await.unwrap();
    assert_eq!(report.started, 1);

    let name = job_name(&operation.id);
    let job = rig.cluster.job(&name).expect("the Job exists");
    assert_eq!(job.spec.operation_id, operation.id);
    assert_eq!(job.spec.values_secrets, ["gantry-values-media"]);

    let stored = rig.store.get(&operation.id).await.unwrap().unwrap();
    assert_eq!(stored.state, State::Running);
    assert_eq!(stored.runner_job.as_deref(), Some(name.as_str()));
    assert_eq!(stored.requested_by, "ana");
}

#[tokio::test]
async fn a_job_manifest_carries_the_plan_and_secret_mounts_but_never_a_secret_value() {
    let rig = Rig::new();
    let gantry = rig.gantry();
    gantry
        .submit("install", "Install media", plan(), "ana")
        .await
        .unwrap();
    tick(&gantry).await.unwrap();

    let name = rig.cluster.job_names().remove(0);
    let manifest = rig.cluster.job(&name).unwrap().spec.manifest();
    let container = &manifest["spec"]["template"]["spec"]["containers"][0];

    assert_eq!(manifest["spec"]["backoffLimit"], 0);
    assert_eq!(
        manifest["spec"]["template"]["spec"]["restartPolicy"],
        "Never"
    );
    assert_eq!(
        manifest["spec"]["template"]["spec"]["serviceAccountName"],
        "gantry-runner"
    );
    assert_eq!(container["command"][0], "gantry-runner");
    assert!(
        container["env"][0]["value"]
            .as_str()
            .unwrap()
            .contains("\"step\":\"install\"")
    );
    assert_eq!(
        container["envFrom"][0]["secretRef"]["name"],
        "gantry-runner-credentials"
    );

    let mounts = container["volumeMounts"].as_array().unwrap();
    assert!(
        mounts
            .iter()
            .any(|m| m["mountPath"] == "/values/gantry-values-media" && m["readOnly"] == true)
    );
    let volumes = manifest["spec"]["template"]["spec"]["volumes"]
        .as_array()
        .unwrap();
    assert!(
        volumes
            .iter()
            .any(|v| v["secret"]["secretName"] == "gantry-values-media")
    );
}

#[tokio::test]
async fn a_restarted_service_adopts_the_runner_a_previous_one_started() {
    let rig = Rig::new();
    let first = rig.gantry();
    let operation = first
        .submit("install", "Install media", plan(), "ana")
        .await
        .unwrap();
    tick(&first).await.unwrap();
    drop(first);

    // The new process knows nothing; the row and the Job are all there is.
    let second = rig.gantry();
    let report = tick(&second).await.unwrap();
    assert_eq!(
        report,
        Report {
            adopted: 1,
            ..Report::default()
        }
    );
    assert_eq!(
        rig.cluster.job_names().len(),
        1,
        "no second Job was created"
    );

    rig.cluster.complete(
        &job_name(&operation.id),
        JobStatus::Succeeded,
        "== step 1/1\nGANTRY-RESULT: ok\n",
    );
    let report = tick(&second).await.unwrap();
    assert_eq!(report.finished, 1);

    let done = rig.store.get(&operation.id).await.unwrap().unwrap();
    assert_eq!(done.state, State::Succeeded);
    assert!(done.log.unwrap().contains("GANTRY-RESULT: ok"));
    assert!(done.finished_at.is_some());
    assert!(
        rig.cluster.job_names().is_empty(),
        "the finished Job is cleared away"
    );
}

#[tokio::test]
async fn a_service_that_died_between_claiming_and_creating_starts_the_runner_on_restart() {
    let rig = Rig::new();
    let gantry = rig.gantry();
    let operation = gantry
        .submit("install", "Install media", plan(), "ana")
        .await
        .unwrap();

    // Claimed (running) but the Job never got created.
    rig.store.claim_next().await.unwrap().unwrap();
    assert!(rig.cluster.job_names().is_empty());

    let report = tick(&gantry).await.unwrap();
    assert_eq!(report.restarted, 1);
    assert!(rig.cluster.job(&job_name(&operation.id)).is_some());
}

#[tokio::test]
async fn failure_and_rollback_are_told_apart_by_the_runners_own_marker() {
    let rig = Rig::new();
    let gantry = rig.gantry();

    let failed = gantry
        .submit("install", "one", plan(), "ana")
        .await
        .unwrap();
    tick(&gantry).await.unwrap();
    rig.cluster.complete(
        &job_name(&failed.id),
        JobStatus::Failed("BackoffLimitExceeded".into()),
        "!! step 1: boom\nGANTRY-RESULT: failed step 1 (install media 1.0.0): boom\n",
    );
    tick(&gantry).await.unwrap();
    let failed = rig.store.get(&failed.id).await.unwrap().unwrap();
    assert_eq!(failed.state, State::Failed);
    assert!(failed.error.unwrap().contains("boom"));

    let undone = gantry
        .submit("upgrade", "two", plan(), "ana")
        .await
        .unwrap();
    tick(&gantry).await.unwrap();
    rig.cluster.complete(
        &job_name(&undone.id),
        JobStatus::Failed("Error".into()),
        "GANTRY-RESULT: rolled_back the new pod never became ready\n",
    );
    tick(&gantry).await.unwrap();
    let undone = rig.store.get(&undone.id).await.unwrap().unwrap();
    assert_eq!(undone.state, State::RolledBack);
    assert!(undone.error.unwrap().contains("never became ready"));
}

#[tokio::test]
async fn a_job_that_failed_without_saying_why_is_still_recorded_as_failed() {
    let rig = Rig::new();
    let gantry = rig.gantry();
    let operation = gantry.submit("install", "x", plan(), "ana").await.unwrap();
    tick(&gantry).await.unwrap();
    rig.cluster.complete(
        &job_name(&operation.id),
        JobStatus::Failed("DeadlineExceeded".into()),
        "",
    );
    tick(&gantry).await.unwrap();

    let stored = rig.store.get(&operation.id).await.unwrap().unwrap();
    assert_eq!(stored.state, State::Failed);
    assert_eq!(stored.error.as_deref(), Some("DeadlineExceeded"));
}

#[tokio::test]
async fn only_one_operation_runs_at_a_time_and_the_next_starts_when_it_ends() {
    let rig = Rig::new();
    let gantry = rig.gantry();
    let first = gantry
        .submit("install", "first", plan(), "ana")
        .await
        .unwrap();
    let second = gantry
        .submit("install", "second", plan(), "ana")
        .await
        .unwrap();

    tick(&gantry).await.unwrap();
    assert_eq!(rig.cluster.job_names().len(), 1);
    assert_eq!(
        rig.store.get(&second.id).await.unwrap().unwrap().state,
        State::Queued
    );

    rig.cluster.complete(
        &job_name(&first.id),
        JobStatus::Succeeded,
        "GANTRY-RESULT: ok\n",
    );
    let report = tick(&gantry).await.unwrap();
    assert_eq!((report.finished, report.started), (1, 1));
    assert_eq!(
        rig.store.get(&second.id).await.unwrap().unwrap().state,
        State::Running
    );
}

#[tokio::test]
async fn cancelling_a_queued_operation_is_immediate_and_a_running_one_stops_its_job() {
    let rig = Rig::new();
    let gantry = rig.gantry();
    let running = gantry
        .submit("install", "running", plan(), "ana")
        .await
        .unwrap();
    let queued = gantry
        .submit("install", "queued", plan(), "ana")
        .await
        .unwrap();
    tick(&gantry).await.unwrap();

    let cancelled = gantry.cancel(&queued.id).await.unwrap().unwrap();
    assert_eq!(cancelled.state, State::Cancelled);

    let flagged = gantry.cancel(&running.id).await.unwrap().unwrap();
    assert_eq!(flagged.state, State::Running);
    assert!(flagged.cancel_requested);

    tick(&gantry).await.unwrap();
    let stopped = rig.store.get(&running.id).await.unwrap().unwrap();
    assert_eq!(stopped.state, State::Cancelled);
    assert!(rig.cluster.job_names().is_empty());
}

#[tokio::test]
async fn an_invalid_plan_is_refused_and_never_queued() {
    let mut rig = Rig::new();
    rig.settings.allowed_namespaces = vec!["scratch".into()];
    let gantry = rig.gantry();

    let error = gantry
        .submit("install", "x", plan(), "ana")
        .await
        .unwrap_err();
    assert!(matches!(error, SubmitError::Invalid(_)));
    assert!(rig.store.list(10).await.unwrap().is_empty());

    assert!(
        gantry
            .submit("install", "  ", Plan::default(), "ana")
            .await
            .is_err()
    );
}

#[tokio::test]
async fn history_is_newest_first() {
    let rig = Rig::new();
    let gantry = rig.gantry();
    gantry
        .submit("install", "first", plan(), "ana")
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    gantry
        .submit("install", "second", plan(), "ana")
        .await
        .unwrap();

    let titles: Vec<String> = rig
        .store
        .list(10)
        .await
        .unwrap()
        .into_iter()
        .map(|o| o.title)
        .collect();
    assert_eq!(titles, ["second", "first"]);
}

#[tokio::test]
async fn every_job_gets_the_age_key_that_opens_a_packages_encrypted_values_as_an_optional_file() {
    let rig = Rig::new();
    let gantry = rig.gantry();
    gantry.submit("install", "x", plan(), "ana").await.unwrap();
    tick(&gantry).await.unwrap();

    let name = rig.cluster.job_names().remove(0);
    let manifest = rig.cluster.job(&name).unwrap().spec.manifest();
    let pod = &manifest["spec"]["template"]["spec"];
    let container = &pod["containers"][0];

    let volume = pod["volumes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["name"] == "age-key")
        .expect("the key is mounted");
    assert_eq!(volume["secret"]["secretName"], "gantry-age-key");
    assert_eq!(
        volume["secret"]["optional"], true,
        "a package with no encrypted values needs no key"
    );
    assert!(
        container["volumeMounts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["mountPath"] == "/keys" && m["readOnly"] == true)
    );
    let env = container["env"].as_array().unwrap();
    assert!(
        env.iter()
            .any(|e| e["name"] == "RIVETER_AGE_KEY_FILE" && e["value"] == "/keys/age.key")
    );
    // The key is a file from a Secret, never a value written into the Job.
    assert!(!manifest.to_string().contains("AGE-SECRET-KEY"));
}
