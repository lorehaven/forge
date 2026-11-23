use gantry_service::domain::cluster::{FakeCluster, Workload};
use gantry_service::domain::cluster_actions::{self, Object};
use gantry_service::domain::executor::{ClusterExecutor, Executor};
use gantry_service::domain::operation::{MemoryStore, Store};
use gantry_service::domain::registry::MemoryRegistry;
use gantry_service::domain::service::Gantry;
use gantry_service::domain::settings::Settings;
use gantry_service::domain::steps::Step;
use std::sync::Arc;

fn rig() -> (Gantry, Arc<MemoryStore>) {
    let settings = Settings::inert();
    let cluster = Arc::new(FakeCluster::new());
    cluster.set_workloads(vec![Workload {
        kind: "deployment".into(),
        name: "web".into(),
        namespace: "ml".into(),
        package: "demo".into(),
        version: None,
        desired: 1,
        ready: 1,
    }]);
    let executor: Arc<dyn Executor> = Arc::new(ClusterExecutor {
        cluster: cluster.clone(),
        settings: settings.clone(),
    });
    let store = Arc::new(MemoryStore::new());
    let gantry = Gantry::new(
        store.clone(),
        cluster,
        executor,
        Arc::new(MemoryRegistry::default()),
        settings,
    );
    (gantry, store)
}

fn web() -> Object {
    Object {
        kind: "Deployment".into(),
        name: "web".into(),
        namespace: Some("ml".into()),
    }
}

async fn steps(store: &MemoryStore, id: &str) -> Vec<Step> {
    store.get(id).await.unwrap().unwrap().plan.steps
}

#[quench_test::test]
async fn deleting_an_object_queues_one_delete_with_the_kind_spelled_for_kubectl() {
    let (gantry, store) = rig();
    let operation = cluster_actions::delete(&gantry, &web(), "ana")
        .await
        .unwrap();
    assert_eq!(operation.kind, "delete");
    match steps(&store, &operation.id).await.as_slice() {
        [
            Step::Delete {
                resource,
                namespace,
            },
        ] => {
            assert_eq!(resource, "deployment.apps/web");
            assert_eq!(namespace.as_deref(), Some("ml"));
        }
        other => panic!("{other:?}"),
    }
}

#[quench_test::test]
async fn what_takes_data_or_the_machine_with_it_is_not_deleted_from_here() {
    let (gantry, _) = rig();
    for kind in ["Namespace", "PersistentVolumeClaim", "Node", "Secret"] {
        let object = Object {
            kind: kind.into(),
            name: "x".into(),
            namespace: None,
        };
        let refused = cluster_actions::delete(&gantry, &object, "ana")
            .await
            .unwrap_err();
        assert!(
            refused.to_string().contains("not deleted"),
            "{kind}: {refused}"
        );
    }
    let missing = Object {
        name: "nothing".into(),
        ..web()
    };
    assert!(
        cluster_actions::delete(&gantry, &missing, "ana")
            .await
            .unwrap_err()
            .to_string()
            .contains("not there")
    );
}

#[quench_test::test]
async fn scaling_sets_the_replicas_and_waits_and_is_bounded() {
    let (gantry, store) = rig();
    let operation = cluster_actions::scale(&gantry, &web(), 3, "ana")
        .await
        .unwrap();
    let steps = steps(&store, &operation.id).await;
    assert!(matches!(&steps[0], Step::Scale { replicas: 3, kind, .. } if kind == "deployment"));
    assert!(matches!(&steps[1], Step::Rollout { .. }));

    assert!(
        cluster_actions::scale(&gantry, &web(), 51, "ana")
            .await
            .is_err()
    );
    let pod = Object {
        kind: "Pod".into(),
        ..web()
    };
    assert!(
        cluster_actions::scale(&gantry, &pod, 1, "ana")
            .await
            .unwrap_err()
            .to_string()
            .contains("cannot be scaled")
    );
}

#[quench_test::test]
async fn an_edit_must_describe_the_object_that_was_opened() {
    let (gantry, store) = rig();
    let ok = "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: web\n  namespace: ml\nspec:\n  replicas: 2\n";
    let operation = cluster_actions::edit(&gantry, &web(), ok, "ana")
        .await
        .unwrap();
    let steps = steps(&store, &operation.id).await;
    assert!(matches!(&steps[0], Step::ApplyYaml { .. }));
    assert!(matches!(&steps[1], Step::Rollout { .. }));

    let other = ok.replace("name: web", "name: db");
    assert!(
        cluster_actions::edit(&gantry, &web(), &other, "ana")
            .await
            .unwrap_err()
            .to_string()
            .contains("Edit the one you opened")
    );
    assert!(
        cluster_actions::edit(&gantry, &web(), ": : nope", "ana")
            .await
            .is_err()
    );
}
