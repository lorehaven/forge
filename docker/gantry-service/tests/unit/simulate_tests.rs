use gantry_service::domain::cluster::{FakeCluster, Workload};
use gantry_service::domain::deployments::{self, Request};
use gantry_service::domain::executor::Executor;
use gantry_service::domain::operation::{MemoryStore, State};
use gantry_service::domain::planner;
use gantry_service::domain::reconciler::tick;
use gantry_service::domain::registry::MemoryRegistry;
use gantry_service::domain::service::Gantry;
use gantry_service::domain::settings::{RunnerMode, Settings};
use gantry_service::domain::simulate::SimulatedExecutor;
use serde_json::json;
use std::sync::Arc;

fn workload(name: &str, desired: i32) -> Workload {
    Workload {
        kind: "deployment".into(),
        name: name.into(),
        namespace: "ml".into(),
        package: "ml".into(),
        version: Some("1.0.0".into()),
        desired,
        ready: desired,
    }
}

fn rig() -> (Arc<FakeCluster>, Arc<MemoryRegistry>, Gantry) {
    let cluster = Arc::new(FakeCluster::new());
    let registry = Arc::new(MemoryRegistry::default());
    for version in ["1.0.0", "1.1.0"] {
        registry.publish_with(
            "ml",
            version,
            Some("ml"),
            json!({"deployment": [
                {"name": "inference", "resources": ["deployment/sage"], "conflicts_with": ["training"]},
                {"name": "training", "resources": ["deployment/trainer"], "default": "stopped"},
            ]}),
        );
    }
    cluster.set_workloads(vec![workload("sage", 1), workload("trainer", 0)]);
    let executor: Arc<dyn Executor> =
        Arc::new(SimulatedExecutor::new(cluster.clone(), registry.clone()));
    let gantry = Gantry::new(
        Arc::new(MemoryStore::new()),
        cluster.clone(),
        executor,
        registry.clone(),
        Settings::inert(),
    );
    (cluster, registry, gantry)
}

async fn run_to_end(gantry: &Gantry, plan_id: &str) -> State {
    let operation = planner::confirm(gantry, plan_id, "ana").await.unwrap();
    tick(gantry).await.unwrap(); // starts it: the simulation finishes at once
    tick(gantry).await.unwrap(); // records the outcome
    gantry
        .store
        .get(&operation.id)
        .await
        .unwrap()
        .unwrap()
        .state
}

#[tokio::test]
async fn a_swap_deletes_one_side_and_applies_the_other_from_the_package() {
    let (cluster, _, gantry) = rig();
    let (plan, _) = deployments::plan(
        &gantry,
        "training",
        &Request::Start { also_stop: vec![] },
        "ana",
    )
    .await
    .unwrap();
    assert_eq!(run_to_end(&gantry, &plan.id).await, State::Succeeded);

    let find = |name: &str| cluster.workloads().into_iter().find(|w| w.name == name);
    assert!(find("sage").is_none(), "stopping is deleting");
    assert_eq!(
        find("trainer").unwrap().desired,
        1,
        "starting applies it from the package"
    );

    let views = deployments::all(&gantry).await.unwrap();
    let state = |name: &str| views.iter().find(|v| v.name == name).unwrap();
    assert_eq!(
        (
            state("inference").desired.as_str(),
            state("training").desired.as_str()
        ),
        ("stopped", "running")
    );
    assert_eq!(
        state("inference").observed,
        gantry_service::domain::deployments::Observed::Stopped
    );
    assert!(
        views.iter().all(|v| !v.drift),
        "what happened matches what was asked"
    );

    // And back again: Gantry knows what the package declares, so it can put Sage back.
    let (plan, _) = deployments::plan(
        &gantry,
        "inference",
        &Request::Start { also_stop: vec![] },
        "ana",
    )
    .await
    .unwrap();
    assert_eq!(run_to_end(&gantry, &plan.id).await, State::Succeeded);
    assert_eq!(find("sage").unwrap().desired, 1);
    assert!(find("trainer").is_none());
}

#[tokio::test]
async fn an_upgrade_moves_what_is_there_and_leaves_a_stopped_deployment_alone() {
    let (cluster, _, gantry) = rig();
    // Training is stopped for real: deleted.
    cluster.delete_resource("deployment", "trainer", Some("ml"));
    gantry
        .store
        .set_deployment_state("training", "stopped", "ana")
        .await
        .unwrap();

    let (plan, _) = planner::plan(&gantry, "ml", None, "ana").await.unwrap();
    assert_eq!(run_to_end(&gantry, &plan.id).await, State::Succeeded);

    let workloads = cluster.workloads();
    assert!(
        workloads
            .iter()
            .all(|w| w.version.as_deref() == Some("1.1.0")),
        "{workloads:?}"
    );
    assert!(
        workloads.iter().all(|w| w.name != "trainer"),
        "the upgrade did not bring training back"
    );
}

#[tokio::test]
async fn a_simulated_operation_has_a_log_in_the_runners_style() {
    let (_, _, gantry) = rig();
    let (plan, _) = deployments::plan(&gantry, "inference", &Request::Stop, "ana")
        .await
        .unwrap();
    let operation = planner::confirm(&gantry, &plan.id, "ana").await.unwrap();
    tick(&gantry).await.unwrap();
    tick(&gantry).await.unwrap();
    let log = gantry
        .store
        .get(&operation.id)
        .await
        .unwrap()
        .unwrap()
        .log
        .unwrap();
    assert!(log.contains("== step 1/"), "{log}");
    assert!(log.contains("deleted"), "{log}");
    assert!(log.trim_end().ends_with("GANTRY-RESULT: ok"), "{log}");
}

#[tokio::test]
async fn an_install_leaves_a_record_of_what_the_package_declares() {
    let (_, _, gantry) = rig();
    let (plan, _) = planner::plan(&gantry, "ml", None, "ana").await.unwrap();
    run_to_end(&gantry, &plan.id).await;

    let inventory = gantry.store.all_inventories().await.unwrap();
    let ml = &inventory["ml"];
    assert_eq!(ml.version, "1.1.0");
    let names: Vec<&str> = ml.resources.iter().map(|r| r.name.as_str()).collect();
    assert!(
        names.contains(&"sage") && names.contains(&"trainer"),
        "{names:?}"
    );
}

#[tokio::test]
async fn a_step_that_cannot_happen_fails_the_operation_with_the_reason() {
    let (cluster, _, gantry) = rig();
    let (plan, _) = deployments::plan(&gantry, "inference", &Request::Stop, "ana")
        .await
        .unwrap();
    // The workload disappears between planning and running.
    cluster.set_workloads(vec![workload("trainer", 0)]);
    let operation = planner::confirm(&gantry, &plan.id, "ana").await;
    // (the stale check may refuse first; either way nothing was recorded as stopped)
    if let Ok(operation) = operation {
        tick(&gantry).await.unwrap();
        tick(&gantry).await.unwrap();
        let done = gantry.store.get(&operation.id).await.unwrap().unwrap();
        assert_eq!(done.state, State::Failed);
        assert!(done.error.unwrap().contains("not found"));
    }
    assert!(gantry.store.deployment_states().await.unwrap().is_empty());
}

#[test]
fn simulating_is_only_allowed_with_the_fake_cluster() {
    let get = |pairs: &'static [(&'static str, &'static str)]| {
        Settings::from_lookup(move |k| {
            pairs
                .iter()
                .find(|(key, _)| *key == k)
                .map(|(_, v)| (*v).to_string())
        })
    };
    let ok = get(&[("GANTRY_CLUSTER", "none"), ("GANTRY_RUNNER", "simulate")]).unwrap();
    assert_eq!(ok.runner, RunnerMode::Simulate);
    assert!(
        get(&[
            ("GANTRY_CLUSTER", "kubeconfig"),
            ("GANTRY_RUNNER", "simulate")
        ])
        .unwrap_err()
        .contains("fake cluster")
    );
}

#[test]
fn the_fake_cluster_can_start_from_a_seed_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("seed.json");
    std::fs::write(
        &path,
        serde_json::to_string(&json!([{
            "kind": "deployment", "name": "sage", "namespace": "ml", "package": "ml",
            "version": "1.0.0", "desired": 1, "ready": 1
        }]))
        .unwrap(),
    )
    .unwrap();

    let cluster = FakeCluster::new();
    cluster.load_seed(&path).unwrap();
    assert_eq!(cluster.workloads().len(), 1);
    assert!(cluster.load_seed(&dir.path().join("missing.json")).is_err());
}
