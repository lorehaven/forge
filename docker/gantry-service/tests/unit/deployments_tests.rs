use gantry_service::domain::cluster::{FakeCluster, JobStatus, Workload};
use gantry_service::domain::deployments::{self, Observed, Request};
use gantry_service::domain::executor::{ClusterExecutor, Executor};
use gantry_service::domain::operation::{MemoryStore, State, job_name};
use gantry_service::domain::planner::{self, Action, PlanError};
use gantry_service::domain::reconciler::tick;
use gantry_service::domain::registry::MemoryRegistry;
use gantry_service::domain::service::{Gantry, SubmitError};
use gantry_service::domain::settings::Settings;
use gantry_service::domain::steps::{Effect, Plan, Step};
use serde_json::json;
use std::sync::Arc;

struct Rig {
    cluster: Arc<FakeCluster>,
    registry: Arc<MemoryRegistry>,
    gantry: Gantry,
}

fn rig() -> Rig {
    let settings = Settings::inert();
    let cluster = Arc::new(FakeCluster::new());
    let registry = Arc::new(MemoryRegistry::default());
    let executor: Arc<dyn Executor> = Arc::new(ClusterExecutor {
        cluster: cluster.clone(),
        settings: settings.clone(),
    });
    let gantry = Gantry::new(
        Arc::new(MemoryStore::new()),
        cluster.clone(),
        executor,
        registry.clone(),
        settings,
    );
    Rig {
        cluster,
        registry,
        gantry,
    }
}

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

/// The user's own example: A (inference) is up by default, B (training) is down, and they conflict.
fn publish_ml(rig: &Rig) {
    rig.registry.publish_with(
        "ml",
        "1.0.0",
        Some("ml"),
        json!({"deployment": [
            {"name": "inference", "resources": ["deployment/sage", "deployment/switchboard"],
             "conflicts_with": ["training"],
             "also_stops": [{"selector": "app=vllm,app.kubernetes.io/managed-by=switchboard"}]},
            {"name": "training", "resources": ["deployment/trainer"], "default": "stopped"},
        ]}),
    );
}

fn inference_up_training_down(rig: &Rig) {
    rig.cluster.set_workloads(vec![
        workload("sage", 1),
        workload("switchboard", 1),
        workload("trainer", 0),
    ]);
}

async fn views(rig: &Rig) -> Vec<deployments::DeploymentView> {
    deployments::all(&rig.gantry).await.unwrap()
}

// ---------------------------------------------------------------- the picture

#[tokio::test]
async fn a_up_b_down_is_what_the_list_shows_and_it_knows_they_conflict() {
    let rig = rig();
    publish_ml(&rig);
    inference_up_training_down(&rig);

    let views = views(&rig).await;
    let a = views.iter().find(|v| v.name == "inference").unwrap();
    let b = views.iter().find(|v| v.name == "training").unwrap();
    assert_eq!(
        (a.observed, a.desired.as_str(), a.drift),
        (Observed::Running, "running", false)
    );
    assert_eq!(
        (b.observed, b.desired.as_str(), b.drift),
        (Observed::Stopped, "stopped", false)
    );
    // Declared on inference only, but it is mutual.
    assert_eq!(b.conflicting_running, ["inference"]);
    assert_eq!(
        a.conflicting_running,
        Vec::<String>::new(),
        "training is down, so it conflicts with nothing now"
    );
}

#[tokio::test]
async fn something_that_differs_from_what_was_asked_is_flagged() {
    let rig = rig();
    publish_ml(&rig);
    // Training was never started, yet it is running.
    rig.cluster.set_workloads(vec![
        workload("sage", 1),
        workload("switchboard", 1),
        workload("trainer", 1),
    ]);
    let views = views(&rig).await;
    assert!(views.iter().find(|v| v.name == "training").unwrap().drift);

    // One of inference's two workloads down: partly running, always drift.
    rig.cluster.set_workloads(vec![
        workload("sage", 0),
        workload("switchboard", 1),
        workload("trainer", 0),
    ]);
    let a = views_of(&rig, "inference").await;
    assert_eq!(a.observed, Observed::Partial);
    assert!(a.drift);
}

async fn views_of(rig: &Rig, name: &str) -> deployments::DeploymentView {
    views(rig)
        .await
        .into_iter()
        .find(|v| v.name == name)
        .unwrap()
}

#[tokio::test]
async fn a_package_that_declares_nothing_is_one_deployment_named_after_it() {
    let rig = rig();
    rig.registry.publish("media", "1.0.0", Some("media"));
    rig.cluster.set_workloads(vec![Workload {
        package: "media".into(),
        namespace: "media".into(),
        ..workload("jellyfin", 1)
    }]);
    let views = views(&rig).await;
    assert_eq!(views.len(), 1);
    assert_eq!(views[0].name, "media");
    assert_eq!(views[0].resources.len(), 1);
}

#[tokio::test]
async fn a_package_that_is_not_installed_shows_its_deployments_as_absent() {
    let rig = rig();
    publish_ml(&rig);
    let views = views(&rig).await;
    assert_eq!(views.len(), 2);
    assert!(
        views
            .iter()
            .all(|v| v.observed == Observed::Absent && !v.drift)
    );
}

// ---------------------------------------------------------------- stop

#[tokio::test]
async fn stopping_goes_down_the_listed_order_clears_unowned_pods_and_waits_for_them() {
    let rig = rig();
    publish_ml(&rig);
    inference_up_training_down(&rig);

    let (stored, action) = deployments::plan(&rig.gantry, "inference", &Request::Stop, "ana")
        .await
        .unwrap();
    assert_eq!(action, Action::Stop);
    let steps = &stored.plan.steps;

    let deletes: Vec<&str> = steps
        .iter()
        .filter_map(|s| match s {
            Step::Delete { resource, .. } => Some(resource.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        deletes,
        ["deployment.apps/sage", "deployment.apps/switchboard"],
        "Sage first: it keeps Switchboard's models warm; stopping is deleting, not scaling to zero"
    );

    let kinds: Vec<&str> = steps
        .iter()
        .map(|s| match s {
            Step::Delete { .. } => "delete",
            Step::DeletePods { .. } => "pods",
            Step::WaitGone { .. } => "wait",
            _ => "other",
        })
        .collect();
    assert_eq!(kinds, ["delete", "delete", "pods", "wait", "wait", "wait"]);
    assert!(
        matches!(&steps[2], Step::DeletePods { selector, .. } if selector.contains("managed-by=switchboard"))
    );

    assert_eq!(
        stored.plan.effects,
        [Effect::SetDesired {
            deployment: "inference".into(),
            desired: "stopped".into()
        }]
    );
    assert_eq!(stored.plan.touches, ["ml"]);
    assert!(
        stored.plan.summary[1].contains("Delete the pods matching"),
        "{:?}",
        stored.plan.summary
    );
}

#[tokio::test]
async fn stopping_what_is_stopped_or_missing_is_refused() {
    let rig = rig();
    publish_ml(&rig);
    inference_up_training_down(&rig);
    let error = deployments::plan(&rig.gantry, "training", &Request::Stop, "ana")
        .await
        .unwrap_err();
    assert!(
        matches!(&error, PlanError::Refused(m) if m.contains("already stopped")),
        "{error}"
    );

    rig.cluster.set_workloads(vec![]);
    let error = deployments::plan(&rig.gantry, "inference", &Request::Stop, "ana")
        .await
        .unwrap_err();
    assert!(
        matches!(&error, PlanError::Refused(m) if m.contains("not installed")),
        "{error}"
    );

    assert!(matches!(
        deployments::plan(&rig.gantry, "nope", &Request::Stop, "ana")
            .await
            .unwrap_err(),
        PlanError::UnknownTarget(_)
    ));
}

#[tokio::test]
async fn what_gantry_stands_on_cannot_be_stopped_by_any_route() {
    let rig = rig();
    rig.registry.publish_with(
        "forge",
        "1.0.0",
        Some("forge"),
        json!({"deployment": [{"name": "everything", "resources": ["deployment/gatehouse", "deployment/sage"]}]}),
    );
    rig.cluster.set_workloads(vec![
        Workload {
            package: "forge".into(),
            namespace: "forge".into(),
            ..workload("gatehouse", 1)
        },
        Workload {
            package: "forge".into(),
            namespace: "forge".into(),
            ..workload("sage", 1)
        },
    ]);

    // Through the planner...
    let error = deployments::plan(&rig.gantry, "everything", &Request::Stop, "ana")
        .await
        .unwrap_err();
    assert!(
        matches!(&error, PlanError::Refused(m) if m.contains("protected")),
        "{error}"
    );

    // ...and through a raw plan posted to the API.
    let raw = Plan {
        steps: vec![Step::Scale {
            namespace: "forge".into(),
            kind: "deployment".into(),
            name: "gantry".into(),
            replicas: 0,
        }],
        ..Plan::default()
    };
    let error = rig
        .gantry
        .submit("custom", "stop myself", raw, "ana")
        .await
        .unwrap_err();
    assert!(
        matches!(&error, SubmitError::Invalid(m) if m.contains("protected")),
        "{error}"
    );

    // Scaling it UP, or upgrading it, is not stopping it.
    let up = Plan {
        steps: vec![Step::Scale {
            namespace: "forge".into(),
            kind: "deployment".into(),
            name: "gantry".into(),
            replicas: 2,
        }],
        ..Plan::default()
    };
    assert!(
        rig.gantry
            .submit("custom", "scale up", up, "ana")
            .await
            .is_ok()
    );
}

// ---------------------------------------------------------------- start and swap

#[tokio::test]
async fn starting_b_plans_the_stop_of_a_first_and_is_a_swap() {
    let rig = rig();
    publish_ml(&rig);
    inference_up_training_down(&rig);

    let (stored, action) = deployments::plan(
        &rig.gantry,
        "training",
        &Request::Start { also_stop: vec![] },
        "ana",
    )
    .await
    .unwrap();
    assert_eq!(action, Action::Swap);
    assert_eq!(action.permission(), "activate");
    assert_eq!(stored.plan.deployments, ["training", "inference"]);

    let order: Vec<&str> = stored
        .plan
        .steps
        .iter()
        .map(|s| match s {
            Step::Scale { .. }
            | Step::Delete { .. }
            | Step::DeletePods { .. }
            | Step::WaitGone { .. } => "stop",
            Step::Pull { .. } | Step::Check { .. } | Step::Install { .. } => "start",
            Step::Rollout { .. } | Step::ApplyYaml { .. } | Step::RestartUsers { .. } => "wait",
        })
        .collect();
    let first_start = order.iter().position(|o| *o == "start").unwrap();
    assert!(
        order[..first_start].iter().all(|o| *o == "stop"),
        "everything is stopped before anything starts: {order:?}"
    );
    assert!(order[first_start..].iter().all(|o| *o != "stop"));

    let Some(Step::Install {
        targets, replicas, ..
    }) = stored
        .plan
        .steps
        .iter()
        .find(|s| matches!(s, Step::Install { .. }))
    else {
        panic!("an install should start it")
    };
    assert_eq!(
        targets,
        &["deployment/trainer"],
        "only B's own workloads are re-applied"
    );
    assert!(
        replicas.is_empty(),
        "so they come up at the package's own counts"
    );

    assert_eq!(
        stored.plan.effects,
        [
            Effect::SetDesired {
                deployment: "inference".into(),
                desired: "stopped".into()
            },
            Effect::SetDesired {
                deployment: "training".into(),
                desired: "running".into()
            },
        ]
    );
}

#[tokio::test]
async fn starting_something_that_conflicts_with_nothing_running_is_a_plain_start() {
    let rig = rig();
    publish_ml(&rig);
    // Both down.
    rig.cluster.set_workloads(vec![
        workload("sage", 0),
        workload("switchboard", 0),
        workload("trainer", 0),
    ]);
    let (stored, action) = deployments::plan(
        &rig.gantry,
        "training",
        &Request::Start { also_stop: vec![] },
        "ana",
    )
    .await
    .unwrap();
    assert_eq!(action, Action::Start);
    assert_eq!(action.permission(), "scale");
    assert!(
        stored
            .plan
            .steps
            .iter()
            .all(|s| !matches!(s, Step::Scale { .. }))
    );
}

#[tokio::test]
async fn an_explicit_also_stop_swaps_things_that_declare_no_conflict() {
    let rig = rig();
    rig.registry.publish_with(
        "a",
        "1.0.0",
        Some("ns"),
        json!({"deployment": [{"name": "one", "resources": ["deployment/one"]}]}),
    );
    rig.registry.publish_with("b", "1.0.0", Some("ns"), json!({"deployment": [{"name": "two", "resources": ["deployment/two"], "default": "stopped"}]}));
    rig.cluster.set_workloads(vec![
        Workload {
            package: "a".into(),
            namespace: "ns".into(),
            ..workload("one", 1)
        },
        Workload {
            package: "b".into(),
            namespace: "ns".into(),
            ..workload("two", 0)
        },
    ]);

    let (stored, action) = deployments::plan(
        &rig.gantry,
        "two",
        &Request::Start {
            also_stop: vec!["one".into()],
        },
        "ana",
    )
    .await
    .unwrap();
    assert_eq!(action, Action::Swap);
    assert_eq!(
        stored.plan.touches,
        ["b", "a"],
        "permission is needed on both packages"
    );
}

#[tokio::test]
async fn starting_what_is_running_is_refused_unless_it_means_stopping_something() {
    let rig = rig();
    publish_ml(&rig);
    inference_up_training_down(&rig);
    let error = deployments::plan(
        &rig.gantry,
        "inference",
        &Request::Start { also_stop: vec![] },
        "ana",
    )
    .await
    .unwrap_err();
    assert!(
        matches!(&error, PlanError::Refused(m) if m.contains("already running")),
        "{error}"
    );
}

#[tokio::test]
async fn starting_from_a_package_at_mixed_versions_is_refused() {
    let rig = rig();
    publish_ml(&rig);
    rig.cluster.set_workloads(vec![
        workload("sage", 1),
        Workload {
            version: Some("0.9.0".into()),
            ..workload("switchboard", 1)
        },
        workload("trainer", 0),
    ]);
    let error = deployments::plan(
        &rig.gantry,
        "training",
        &Request::Start { also_stop: vec![] },
        "ana",
    )
    .await
    .unwrap_err();
    assert!(
        matches!(&error, PlanError::Refused(m) if m.contains("more than one version")),
        "{error}"
    );
}

// ---------------------------------------------------------------- recorded state

#[tokio::test]
async fn the_recorded_state_changes_only_when_the_operation_succeeds() {
    let rig = rig();
    publish_ml(&rig);
    inference_up_training_down(&rig);

    let (stored, _) = deployments::plan(&rig.gantry, "inference", &Request::Stop, "ana")
        .await
        .unwrap();
    let operation = planner::confirm(&rig.gantry, &stored.id, "ana")
        .await
        .unwrap();
    tick(&rig.gantry).await.unwrap();
    assert!(
        rig.gantry
            .store
            .deployment_states()
            .await
            .unwrap()
            .is_empty(),
        "nothing is recorded while it runs"
    );

    rig.cluster.complete(
        &job_name(&operation.id),
        JobStatus::Failed("boom".into()),
        "GANTRY-RESULT: failed boom\n",
    );
    tick(&rig.gantry).await.unwrap();
    assert!(
        rig.gantry
            .store
            .deployment_states()
            .await
            .unwrap()
            .is_empty(),
        "a failure records nothing"
    );

    // Try again; this time it works.
    let (stored, _) = deployments::plan(&rig.gantry, "inference", &Request::Stop, "ana")
        .await
        .unwrap();
    let operation = planner::confirm(&rig.gantry, &stored.id, "ana")
        .await
        .unwrap();
    tick(&rig.gantry).await.unwrap();
    rig.cluster.complete(
        &job_name(&operation.id),
        JobStatus::Succeeded,
        "GANTRY-RESULT: ok\n",
    );
    tick(&rig.gantry).await.unwrap();

    assert_eq!(
        rig.gantry
            .store
            .get(&operation.id)
            .await
            .unwrap()
            .unwrap()
            .state,
        State::Succeeded
    );
    assert_eq!(
        rig.gantry.store.deployment_states().await.unwrap()["inference"],
        "stopped"
    );
}

#[tokio::test]
async fn an_update_of_a_stopped_deployment_keeps_it_stopped() {
    let rig = rig();
    publish_ml(&rig);
    rig.registry.publish_with(
        "ml",
        "1.1.0",
        Some("ml"),
        json!({"deployment": [
            {"name": "inference", "resources": ["deployment/sage", "deployment/switchboard"]},
            {"name": "training", "resources": ["deployment/trainer"], "default": "stopped"},
        ]}),
    );
    inference_up_training_down(&rig);

    let (stored, action) = planner::plan(&rig.gantry, "ml", None, "ana").await.unwrap();
    assert_eq!(action, Action::Upgrade);
    let Step::Install {
        except, targets, ..
    } = &stored.plan.steps[2]
    else {
        panic!("install expected")
    };
    assert!(targets.is_empty(), "the whole package is updated");
    assert_eq!(
        except,
        &["deployment/trainer"],
        "but training is left out, so it stays down"
    );
    assert!(
        stored
            .plan
            .summary
            .iter()
            .any(|l| l.contains("training is stopped")),
        "{:?}",
        stored.plan.summary
    );

    // Once inference has been stopped on purpose, an update keeps that too.
    rig.gantry
        .store
        .set_deployment_state("inference", "stopped", "ana")
        .await
        .unwrap();
    let (stored, _) = planner::plan(&rig.gantry, "ml", None, "ana").await.unwrap();
    let Step::Install { except, .. } = &stored.plan.steps[2] else {
        panic!("install expected")
    };
    assert!(except.contains(&"deployment/sage".to_string()));
    assert!(except.contains(&"deployment/switchboard".to_string()));

    // And starting training deliberately lifts its default hold.
    rig.gantry
        .store
        .set_deployment_state("training", "running", "ana")
        .await
        .unwrap();
    let (stored, _) = planner::plan(&rig.gantry, "ml", None, "ana").await.unwrap();
    let Step::Install { except, .. } = &stored.plan.steps[2] else {
        panic!("install expected")
    };
    assert!(!except.contains(&"deployment/trainer".to_string()));
}

#[tokio::test]
async fn a_first_install_of_a_package_whose_deployment_defaults_to_stopped_leaves_it_out() {
    let rig = rig();
    publish_ml(&rig);
    let (stored, action) = planner::plan(&rig.gantry, "ml", None, "ana").await.unwrap();
    assert_eq!(action, Action::Install);
    let Step::Install { except, .. } = &stored.plan.steps[2] else {
        panic!("install expected")
    };
    assert_eq!(
        except,
        &["deployment/trainer"],
        "left out of the install, so it is not created"
    );
}

#[tokio::test]
async fn a_plan_is_refused_if_the_deployments_moved_since_it_was_made() {
    let rig = rig();
    publish_ml(&rig);
    inference_up_training_down(&rig);
    let (stored, _) = deployments::plan(
        &rig.gantry,
        "training",
        &Request::Start { also_stop: vec![] },
        "ana",
    )
    .await
    .unwrap();

    // Someone stopped inference by hand in the meantime.
    rig.cluster.set_workloads(vec![
        workload("sage", 0),
        workload("switchboard", 0),
        workload("trainer", 0),
    ]);

    let error = planner::confirm(&rig.gantry, &stored.id, "ana")
        .await
        .unwrap_err();
    assert!(matches!(&error, PlanError::Stale(_)), "{error}");
    assert!(rig.gantry.store.list(10).await.unwrap().is_empty());
}
