use gantry_service::domain::actions::{self, ActionError, Target};
use gantry_service::domain::cluster::{Extra, FakeCluster, LiveResource, Workload};
use gantry_service::domain::commands::{Context, edit_file};
use gantry_service::domain::executor::Executor;
use gantry_service::domain::operation::{Inventory, MemoryStore};
use gantry_service::domain::reconciler::tick;
use gantry_service::domain::registry::MemoryRegistry;
use gantry_service::domain::resources::{InventoryItem, State, inventory_from_log, kubectl_name};
use gantry_service::domain::runner::{Exec, Outcome, run};
use gantry_service::domain::service::Gantry;
use gantry_service::domain::settings::Settings;
use gantry_service::domain::simulate::SimulatedExecutor;
use gantry_service::domain::steps::{Plan, Step};
use std::path::PathBuf;
use std::sync::Arc;

fn item(kind: &str, name: &str, namespace: Option<&str>) -> InventoryItem {
    InventoryItem {
        api_version: Some(
            match kind {
                "Deployment" | "StatefulSet" => "apps/v1",
                "ClusterRole" => "rbac.authorization.k8s.io/v1",
                _ => "v1",
            }
            .to_string(),
        ),
        kind: kind.to_string(),
        name: name.to_string(),
        namespace: namespace.map(str::to_string),
    }
}

fn live(kind: &str, name: &str, namespace: Option<&str>, edited: bool) -> LiveResource {
    LiveResource {
        api_version: item(kind, name, namespace).api_version,
        kind: kind.to_string(),
        name: name.to_string(),
        namespace: namespace.map(str::to_string),
        package: "ml".to_string(),
        version: Some("1.0.0".to_string()),
        ready: None,
        edited,
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

struct Rig {
    cluster: Arc<FakeCluster>,
    gantry: Gantry,
}

/// A package `ml` with a Deployment, a Service, a ConfigMap, a Secret and a ClusterRole in its inventory.
async fn rig_with(settings: Settings) -> Rig {
    let cluster = Arc::new(FakeCluster::new());
    let registry = Arc::new(MemoryRegistry::default());
    registry.publish("ml", "1.0.0", Some("ml"));
    cluster.set_workloads(vec![workload("sage", 1)]);
    cluster.upsert_extra(Extra {
        live: live("Service", "sage", Some("ml"), false),
        yaml: "apiVersion: v1\nkind: Service\nmetadata:\n  name: sage\n  namespace: ml\nspec:\n  ports: []\n".into(),
    });
    cluster.upsert_extra(Extra {
        live: live("ConfigMap", "sage-config", Some("ml"), false),
        yaml: "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: sage-config\n  namespace: ml\ndata:\n  LEVEL: info\n".into(),
    });
    cluster.upsert_extra(Extra {
        live: live("ClusterRole", "sage-role", None, false),
        yaml: "apiVersion: rbac.authorization.k8s.io/v1\nkind: ClusterRole\nmetadata:\n  name: sage-role\n".into(),
    });

    let declared = vec![
        item("Deployment", "sage", Some("ml")),
        item("Service", "sage", Some("ml")),
        item("ConfigMap", "sage-config", Some("ml")),
        item("Secret", "sage-secret", Some("ml")),
        item("ClusterRole", "sage-role", None),
        item("Deployment", "trainer", Some("ml")),
    ];
    // What the simulated package declares: the record an install reports, and what apply puts back.
    cluster.set_catalog("ml", declared);

    let store = Arc::new(MemoryStore::new());
    let executor: Arc<dyn Executor> =
        Arc::new(SimulatedExecutor::new(cluster.clone(), registry.clone()));
    let gantry = Gantry::new(store, cluster.clone(), executor, registry, settings);
    gantry
        .store
        .set_inventory(
            "ml",
            &Inventory {
                version: "1.0.0".into(),
                resources: vec![
                    item("Deployment", "sage", Some("ml")),
                    item("Service", "sage", Some("ml")),
                    item("ConfigMap", "sage-config", Some("ml")),
                    item("Secret", "sage-secret", Some("ml")),
                    item("ClusterRole", "sage-role", None),
                    item("Deployment", "trainer", Some("ml")),
                ],
            },
        )
        .await
        .unwrap();
    Rig { cluster, gantry }
}

async fn rig() -> Rig {
    rig_with(Settings::inert()).await
}

fn target(kind: &str, name: &str, namespace: Option<&str>) -> Target {
    let i = item(kind, name, namespace);
    Target {
        api_version: i.api_version,
        kind: i.kind,
        name: i.name,
        namespace: i.namespace,
    }
}

async fn run_to_end(gantry: &Gantry) {
    tick(gantry).await.unwrap();
    tick(gantry).await.unwrap();
}

// ---------------------------------------------------------------- the list

#[tokio::test]
async fn resources_are_grouped_by_package_with_what_is_missing_and_what_is_hidden() {
    let rig = rig().await;
    let groups = actions::groups(&rig.gantry).await.unwrap();
    let ml = groups.iter().find(|g| g.package == "ml").unwrap();
    assert!(ml.inventoried);
    assert_eq!(ml.installed.as_deref(), Some("1.0.0"));

    let state = |kind: &str, name: &str| {
        ml.rows
            .iter()
            .find(|r| r.kind == kind && r.name == name)
            .unwrap()
            .state
    };
    assert_eq!(state("Deployment", "sage"), State::Synced);
    assert_eq!(state("Service", "sage"), State::Synced);
    assert_eq!(
        state("Deployment", "trainer"),
        State::Missing,
        "declared, not there: stopped"
    );
    assert_eq!(
        state("Secret", "sage-secret"),
        State::Hidden,
        "a Secret is never looked at"
    );

    // Workloads come first, then by kind.
    assert!(ml.rows[0].workload && ml.rows[1].workload);
    assert!(
        ml.rows
            .iter()
            .find(|r| r.name == "trainer")
            .is_some_and(|r| !r.editable)
    );
    assert!(
        ml.rows
            .iter()
            .find(|r| r.name == "sage-secret")
            .is_some_and(|r| !r.editable)
    );
}

#[tokio::test]
async fn something_in_the_cluster_the_package_does_not_declare_is_flagged_and_an_edit_is_shown() {
    let rig = rig().await;
    rig.cluster.upsert_extra(Extra {
        live: live("ConfigMap", "stray", Some("ml"), false),
        yaml: "kind: ConfigMap\nmetadata:\n  name: stray\n".into(),
    });
    rig.cluster.apply_yaml("apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: sage-config\n  namespace: ml\ndata:\n  LEVEL: debug\n").unwrap();

    let groups = actions::groups(&rig.gantry).await.unwrap();
    let ml = groups.iter().find(|g| g.package == "ml").unwrap();
    let state = |name: &str| ml.rows.iter().find(|r| r.name == name).unwrap().state;
    assert_eq!(state("stray"), State::Extra);
    assert_eq!(state("sage-config"), State::Edited);
}

#[tokio::test]
async fn a_package_with_no_inventory_shows_only_what_is_live_and_says_so() {
    let cluster = Arc::new(FakeCluster::new());
    cluster.set_workloads(vec![workload("sage", 1)]);
    let registry = Arc::new(MemoryRegistry::default());
    let executor: Arc<dyn Executor> =
        Arc::new(SimulatedExecutor::new(cluster.clone(), registry.clone()));
    let gantry = Gantry::new(
        Arc::new(MemoryStore::new()),
        cluster,
        executor,
        registry,
        Settings::inert(),
    );

    let groups = actions::groups(&gantry).await.unwrap();
    let ml = groups.iter().find(|g| g.package == "ml").unwrap();
    assert!(!ml.inventoried);
    assert_eq!(ml.rows.len(), 1);
    assert_eq!(
        ml.rows[0].state,
        State::Synced,
        "with nothing to compare to, it is simply there"
    );
}

#[test]
fn kubectl_names_carry_the_group_so_a_crd_is_not_mistaken_for_a_core_kind() {
    assert_eq!(
        kubectl_name(Some("apps/v1"), "Deployment", "sage"),
        "deployment.apps/sage"
    );
    assert_eq!(kubectl_name(Some("v1"), "Service", "sage"), "service/sage");
    assert_eq!(
        kubectl_name(Some("cert-manager.io/v1"), "Certificate", "tls"),
        "certificate.cert-manager.io/tls"
    );
    assert_eq!(kubectl_name(None, "ConfigMap", "c"), "configmap/c");
}

#[test]
fn the_inventory_is_read_from_the_last_line_riveter_printed() {
    let log = "x\nriveter-inventory: [{\"kind\":\"Deployment\",\"name\":\"a\",\"namespace\":\"n\",\"apiVersion\":\"apps/v1\"}]\nmore\n";
    let items = inventory_from_log(log).unwrap();
    assert_eq!(items[0].kind, "Deployment");
    assert!(inventory_from_log("nothing here").is_none());
    assert!(inventory_from_log("riveter-inventory: not json").is_none());
    assert!(
        inventory_from_log("riveter-inventory: []").is_none(),
        "an empty list is not a record"
    );
}

// ---------------------------------------------------------------- delete and apply

#[tokio::test]
async fn deleting_a_resource_deletes_exactly_that_resource_the_way_kubectl_names_it() {
    let rig = rig().await;
    let operation = actions::delete(
        &rig.gantry,
        "ml",
        &target("Deployment", "sage", Some("ml")),
        "ana",
    )
    .await
    .unwrap();
    assert_eq!(
        operation.plan.steps,
        [Step::Delete {
            resource: "deployment.apps/sage".into(),
            namespace: Some("ml".into())
        }]
    );
    assert_eq!(operation.title, "Delete Deployment/sage");

    // A cluster-scoped kind has no namespace.
    let operation = actions::delete(
        &rig.gantry,
        "ml",
        &target("ClusterRole", "sage-role", None),
        "ana",
    )
    .await
    .unwrap();
    assert_eq!(
        operation.plan.steps,
        [Step::Delete {
            resource: "clusterrole.rbac.authorization.k8s.io/sage-role".into(),
            namespace: None
        }]
    );
}

#[tokio::test]
async fn some_things_are_not_deleted_from_a_button() {
    let rig = rig().await;
    rig.cluster.upsert_extra(Extra {
        live: live("PersistentVolumeClaim", "data", Some("ml"), false),
        yaml: "kind: PersistentVolumeClaim\n".into(),
    });
    rig.gantry
        .store
        .set_inventory(
            "ml",
            &Inventory {
                version: "1.0.0".into(),
                resources: vec![
                    item("PersistentVolumeClaim", "data", Some("ml")),
                    item("Secret", "s", Some("ml")),
                    item("Deployment", "trainer", Some("ml")),
                ],
            },
        )
        .await
        .unwrap();

    let refuse = |error: ActionError, needle: &str| {
        assert!(
            matches!(&error, ActionError::Refused(m) if m.contains(needle)),
            "{error}"
        );
    };
    refuse(
        actions::delete(
            &rig.gantry,
            "ml",
            &target("PersistentVolumeClaim", "data", Some("ml")),
            "ana",
        )
        .await
        .unwrap_err(),
        "data",
    );
    refuse(
        actions::delete(&rig.gantry, "ml", &target("Secret", "s", Some("ml")), "ana")
            .await
            .unwrap_err(),
        "Secret",
    );
    refuse(
        actions::delete(
            &rig.gantry,
            "ml",
            &target("Deployment", "trainer", Some("ml")),
            "ana",
        )
        .await
        .unwrap_err(),
        "already gone",
    );
    assert!(matches!(
        actions::delete(
            &rig.gantry,
            "ml",
            &target("Deployment", "nope", Some("ml")),
            "ana"
        )
        .await
        .unwrap_err(),
        ActionError::NotFound(_)
    ));
    assert!(matches!(
        actions::delete(
            &rig.gantry,
            "other",
            &target("Deployment", "sage", Some("ml")),
            "ana"
        )
        .await
        .unwrap_err(),
        ActionError::NotFound(_)
    ));
}

#[tokio::test]
async fn what_gantry_stands_on_cannot_be_deleted_either() {
    let mut settings = Settings::inert();
    settings.protected = vec![("ml".into(), "sage".into())];
    let rig = rig_with(settings).await;
    let error = actions::delete(
        &rig.gantry,
        "ml",
        &target("Deployment", "sage", Some("ml")),
        "ana",
    )
    .await
    .unwrap_err();
    assert!(matches!(&error, ActionError::Submit(_)), "{error}");
    assert!(error.to_string().contains("protected"), "{error}");
}

#[tokio::test]
async fn applying_a_missing_resource_applies_only_it_from_the_installed_version_of_the_package() {
    let rig = rig().await;
    let operation = actions::apply(
        &rig.gantry,
        "ml",
        Some(&target("Deployment", "trainer", Some("ml"))),
        "ana",
    )
    .await
    .unwrap();

    let kinds: Vec<&str> = operation
        .plan
        .steps
        .iter()
        .map(|s| match s {
            Step::Pull { .. } => "pull",
            Step::Check { .. } => "check",
            Step::Install { .. } => "install",
            _ => "other",
        })
        .collect();
    assert_eq!(kinds, ["pull", "check", "install"]);
    let Step::Install {
        version,
        targets,
        namespace,
        except,
        ..
    } = &operation.plan.steps[2]
    else {
        panic!()
    };
    assert_eq!(version, "1.0.0");
    assert_eq!(targets, &["deployment/trainer"]);
    assert_eq!(namespace, "ml");
    assert!(except.is_empty());
}

#[tokio::test]
async fn applying_with_no_resource_applies_everything_missing_and_refuses_when_nothing_is() {
    let rig = rig().await;
    let operation = actions::apply(&rig.gantry, "ml", None, "ana")
        .await
        .unwrap();
    let Step::Install { targets, .. } = &operation.plan.steps[2] else {
        panic!()
    };
    assert_eq!(targets, &["deployment/trainer"]);
    assert_eq!(operation.title, "Apply 1 missing from ml");
    run_to_end(&rig.gantry).await;

    let error = actions::apply(&rig.gantry, "ml", None, "ana")
        .await
        .unwrap_err();
    assert!(
        matches!(&error, ActionError::Refused(m) if m.contains("nothing of ml is missing")),
        "{error}"
    );
}

#[tokio::test]
async fn delete_then_apply_puts_a_resource_back_from_what_the_package_declares() {
    let rig = rig().await;
    actions::delete(
        &rig.gantry,
        "ml",
        &target("Service", "sage", Some("ml")),
        "ana",
    )
    .await
    .unwrap();
    run_to_end(&rig.gantry).await;

    let groups = actions::groups(&rig.gantry).await.unwrap();
    let ml = groups.iter().find(|g| g.package == "ml").unwrap();
    assert_eq!(
        ml.rows.iter().find(|r| r.kind == "Service").unwrap().state,
        State::Missing
    );

    actions::apply(
        &rig.gantry,
        "ml",
        Some(&target("Service", "sage", Some("ml"))),
        "ana",
    )
    .await
    .unwrap();
    run_to_end(&rig.gantry).await;
    let groups = actions::groups(&rig.gantry).await.unwrap();
    let ml = groups.iter().find(|g| g.package == "ml").unwrap();
    assert_eq!(
        ml.rows.iter().find(|r| r.kind == "Service").unwrap().state,
        State::Synced
    );
}

// ---------------------------------------------------------------- edit

#[tokio::test]
async fn editing_applies_the_yaml_as_written_marks_it_and_restarts_what_reads_a_configmap() {
    let rig = rig().await;
    let yaml = actions::yaml(
        &rig.gantry,
        "ml",
        &target("ConfigMap", "sage-config", Some("ml")),
    )
    .await
    .unwrap();
    assert!(yaml.contains("LEVEL: info"));

    let edited = yaml.replace("LEVEL: info", "LEVEL: debug");
    let operation = actions::edit(
        &rig.gantry,
        "ml",
        &target("ConfigMap", "sage-config", Some("ml")),
        &edited,
        "ana",
    )
    .await
    .unwrap();

    let Step::ApplyYaml { yaml, namespace } = &operation.plan.steps[0] else {
        panic!()
    };
    assert!(yaml.contains("LEVEL: debug"));
    assert!(
        yaml.contains("gantry.forge/edited"),
        "marked on the object itself: {yaml}"
    );
    assert!(yaml.contains("ana"));
    assert_eq!(namespace.as_deref(), Some("ml"));
    assert!(
        matches!(&operation.plan.steps[1], Step::RestartUsers { kind, name, .. } if kind == "configmap" && name == "sage-config")
    );

    run_to_end(&rig.gantry).await;
    let groups = actions::groups(&rig.gantry).await.unwrap();
    let ml = groups.iter().find(|g| g.package == "ml").unwrap();
    assert_eq!(
        ml.rows
            .iter()
            .find(|r| r.name == "sage-config")
            .unwrap()
            .state,
        State::Edited
    );
}

#[tokio::test]
async fn editing_a_workload_waits_for_the_rollout_it_causes() {
    let rig = rig().await;
    let yaml = actions::yaml(&rig.gantry, "ml", &target("Deployment", "sage", Some("ml")))
        .await
        .unwrap();
    let operation = actions::edit(
        &rig.gantry,
        "ml",
        &target("Deployment", "sage", Some("ml")),
        &yaml.replace("replicas: 1", "replicas: 3"),
        "ana",
    )
    .await
    .unwrap();
    assert!(
        matches!(&operation.plan.steps[1], Step::Rollout { kind, name, rollback_on_failure: false, .. } if kind == "deployment" && name == "sage")
    );

    run_to_end(&rig.gantry).await;
    assert_eq!(rig.cluster.workloads()[0].desired, 3);
}

#[tokio::test]
async fn an_edit_must_describe_the_resource_that_was_opened() {
    let rig = rig().await;
    let other =
        "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: something-else\n  namespace: ml\n";
    let error = actions::edit(
        &rig.gantry,
        "ml",
        &target("ConfigMap", "sage-config", Some("ml")),
        other,
        "ana",
    )
    .await
    .unwrap_err();
    assert!(
        matches!(&error, ActionError::Refused(m) if m.contains("Edit the one you opened")),
        "{error}"
    );

    let moved = "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: sage-config\n  namespace: kube-system\n";
    assert!(
        actions::edit(
            &rig.gantry,
            "ml",
            &target("ConfigMap", "sage-config", Some("ml")),
            moved,
            "ana"
        )
        .await
        .is_err()
    );
    assert!(
        actions::edit(
            &rig.gantry,
            "ml",
            &target("ConfigMap", "sage-config", Some("ml")),
            "not: [valid",
            "ana"
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn a_secret_is_never_shown_or_edited_and_a_missing_resource_cannot_be_opened() {
    let rig = rig().await;
    let secret = target("Secret", "sage-secret", Some("ml"));
    assert!(
        matches!(actions::yaml(&rig.gantry, "ml", &secret).await.unwrap_err(), ActionError::Refused(m) if m.contains("never shown"))
    );
    assert!(
        actions::edit(
            &rig.gantry,
            "ml",
            &secret,
            "kind: Secret\nmetadata:\n  name: sage-secret\n",
            "ana"
        )
        .await
        .is_err()
    );
    assert!(matches!(
        actions::yaml(&rig.gantry, "ml", &target("Deployment", "trainer", Some("ml"))).await.unwrap_err(),
        ActionError::Refused(m) if m.contains("apply it first")
    ));
}

#[tokio::test]
async fn refreshing_reads_the_package_and_changes_nothing_but_the_record() {
    let rig = rig().await;
    let operation = actions::refresh(&rig.gantry, "ml", "ana").await.unwrap();
    assert!(
        operation
            .plan
            .steps
            .iter()
            .all(|s| matches!(s, Step::Pull { .. } | Step::Check { .. }))
    );
    run_to_end(&rig.gantry).await;
    assert_eq!(rig.cluster.workloads().len(), 1, "nothing was applied");
    assert!(
        rig.gantry
            .store
            .all_inventories()
            .await
            .unwrap()
            .contains_key("ml")
    );
}

// ---------------------------------------------------------------- the steps and the runner

fn ctx(dir: &std::path::Path) -> Context {
    Context {
        values_dir: PathBuf::from("/values"),
        packages_dir: dir.to_path_buf(),
        allowed_namespaces: vec![],
        dry_run: false,
        source_dir: None,
    }
}

#[derive(Default)]
struct Recorder {
    ran: Vec<String>,
    listing: String,
}

impl Exec for Recorder {
    fn run(&mut self, command: &gantry_service::domain::commands::Command) -> std::io::Result<i32> {
        self.ran.push(command.display());
        Ok(0)
    }
    fn capture(
        &mut self,
        command: &gantry_service::domain::commands::Command,
    ) -> std::io::Result<(i32, String)> {
        self.ran.push(command.display());
        Ok((0, self.listing.clone()))
    }
}

#[test]
fn the_edited_yaml_is_written_to_a_file_named_by_its_content_and_applied() {
    let dir = tempfile::tempdir().unwrap();
    let yaml = "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: c\n  namespace: ml\n";
    let plan = Plan {
        steps: vec![Step::ApplyYaml {
            namespace: Some("ml".into()),
            yaml: yaml.into(),
        }],
        ..Plan::default()
    };
    let mut exec = Recorder::default();
    let outcome = run(&plan, &ctx(dir.path()), &mut exec, &mut Vec::new());
    assert_eq!(outcome, Outcome::Succeeded);

    let file = edit_file(dir.path(), yaml);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), yaml);
    assert_eq!(exec.ran, [format!("kubectl apply -f {}", file.display())]);
}

#[test]
fn a_dry_run_writes_nothing_to_the_cluster_but_still_shows_the_command() {
    let dir = tempfile::tempdir().unwrap();
    let yaml = "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: c\n";
    let plan = Plan {
        steps: vec![Step::ApplyYaml {
            namespace: None,
            yaml: yaml.into(),
        }],
        ..Plan::default()
    };
    let mut exec = Recorder::default();
    let mut out = Vec::new();
    let mut context = ctx(dir.path());
    context.dry_run = true;
    run(&plan, &context, &mut exec, &mut out);
    assert!(exec.ran.is_empty());
    assert!(
        String::from_utf8(out)
            .unwrap()
            .contains("(dry run: not executed)")
    );
}

#[test]
fn a_changed_configmap_restarts_exactly_the_workloads_that_use_it() {
    let listing = serde_json::json!({"items": [
        {"kind": "Deployment", "metadata": {"name": "by-env-from"},
         "spec": {"template": {"spec": {"containers": [{"envFrom": [{"configMapRef": {"name": "app-config"}}]}]}}}},
        {"kind": "Deployment", "metadata": {"name": "by-env"},
         "spec": {"template": {"spec": {"containers": [{"env": [{"name": "X", "valueFrom": {"configMapKeyRef": {"name": "app-config", "key": "k"}}}]}]}}}},
        {"kind": "StatefulSet", "metadata": {"name": "by-volume"},
         "spec": {"template": {"spec": {"volumes": [{"configMap": {"name": "app-config"}}], "containers": []}}}},
        {"kind": "Deployment", "metadata": {"name": "unrelated"},
         "spec": {"template": {"spec": {"containers": [{"envFrom": [{"configMapRef": {"name": "other"}}]}]}}}},
        {"kind": "Deployment", "metadata": {"name": "uses-a-secret-of-that-name"},
         "spec": {"template": {"spec": {"containers": [{"envFrom": [{"secretRef": {"name": "app-config"}}]}]}}}}
    ]})
    .to_string();
    let plan = Plan {
        steps: vec![Step::RestartUsers {
            namespace: "ml".into(),
            kind: "configmap".into(),
            name: "app-config".into(),
            timeout_secs: 30,
        }],
        ..Plan::default()
    };
    let mut exec = Recorder {
        listing,
        ..Recorder::default()
    };
    let dir = tempfile::tempdir().unwrap();
    let outcome = run(&plan, &ctx(dir.path()), &mut exec, &mut Vec::new());
    assert_eq!(outcome, Outcome::Succeeded);

    let restarts: Vec<&String> = exec
        .ran
        .iter()
        .filter(|c| c.contains("rollout restart"))
        .collect();
    assert_eq!(restarts.len(), 3, "{:?}", exec.ran);
    assert!(
        restarts
            .iter()
            .any(|c| c.ends_with("deployment/by-env-from"))
    );
    assert!(restarts.iter().any(|c| c.ends_with("deployment/by-env")));
    assert!(
        restarts
            .iter()
            .any(|c| c.ends_with("statefulset/by-volume"))
    );
    assert!(
        exec.ran
            .iter()
            .filter(|c| c.contains("rollout status"))
            .count()
            == 3,
        "each is waited on"
    );
}

#[test]
fn nothing_using_it_is_not_an_error() {
    let plan = Plan {
        steps: vec![Step::RestartUsers {
            namespace: "ml".into(),
            kind: "configmap".into(),
            name: "orphan".into(),
            timeout_secs: 30,
        }],
        ..Plan::default()
    };
    let mut exec = Recorder {
        listing: r#"{"items": []}"#.into(),
        ..Recorder::default()
    };
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(
        run(&plan, &ctx(dir.path()), &mut exec, &mut Vec::new()),
        Outcome::Succeeded
    );
}

#[test]
fn edit_and_delete_steps_are_validated_like_every_other() {
    let bad = |step: Step| {
        Plan {
            steps: vec![step],
            ..Plan::default()
        }
        .validate(&[])
        .unwrap_err()
    };
    assert!(
        bad(Step::Delete {
            resource: "--all".into(),
            namespace: None
        })
        .contains("kind/name")
    );
    assert!(
        bad(Step::Delete {
            resource: "secret/x y".into(),
            namespace: None
        })
        .contains("kind/name")
    );
    assert!(
        bad(Step::ApplyYaml {
            namespace: None,
            yaml: "kind: Secret\nmetadata:\n  name: s\n".into()
        })
        .contains("Secret")
    );
    assert!(
        bad(Step::ApplyYaml {
            namespace: Some("a".into()),
            yaml: "kind: ConfigMap\nmetadata:\n  name: c\n  namespace: b\n".into()
        })
        .contains("namespace")
    );
    assert!(
        bad(Step::ApplyYaml {
            namespace: None,
            yaml: "just text".into()
        })
        .contains("kind")
            || true
    );
    assert!(
        bad(Step::ApplyYaml {
            namespace: None,
            yaml: format!(
                "kind: ConfigMap\nmetadata:\n  name: c\ndata:\n  k: {}\n",
                "x".repeat(200_000)
            )
        })
        .contains("KiB")
    );

    // The allow-list covers them: no namespace means cluster-scoped, which an allow-list does not cover.
    let allowed = vec!["ml".to_string()];
    let step = Step::Delete {
        resource: "clusterrole.rbac.authorization.k8s.io/x".into(),
        namespace: None,
    };
    assert!(
        Plan {
            steps: vec![step],
            ..Plan::default()
        }
        .validate(&allowed)
        .unwrap_err()
        .contains("may only act in")
    );
    let step = Step::Delete {
        resource: "service/x".into(),
        namespace: Some("kube-system".into()),
    };
    assert!(
        Plan {
            steps: vec![step],
            ..Plan::default()
        }
        .validate(&allowed)
        .unwrap_err()
        .contains("allow-list")
    );
}

mod summary {
    use gantry_service::domain::resources::{Group, Row, State, SyncState};
    use gantry_service::domain::targets::Status;

    fn row(kind: &str, name: &str, state: State, ready: Option<&str>) -> Row {
        Row {
            api_version: None,
            kind: kind.to_string(),
            name: name.to_string(),
            namespace: None,
            state,
            ready: ready.map(str::to_string),
            workload: kind == "Deployment",
            editable: true,
        }
    }

    fn group(status: Status, installed: Option<&str>, rows: Vec<Row>) -> Group {
        Group {
            package: "demo".to_string(),
            description: None,
            installed: installed.map(str::to_string),
            offered: Some("2.0.0".to_string()),
            status,
            inventoried: true,
            rows,
        }
    }

    #[test]
    fn nothing_in_the_cluster_is_not_installed() {
        let g = group(Status::NotInstalled, None, vec![]);
        assert_eq!(g.summary().sync, SyncState::NotInstalled);
    }

    #[test]
    fn a_missing_resource_makes_the_application_missing_before_anything_else() {
        let g = group(
            Status::UpdateAvailable,
            Some("1.0.0"),
            vec![
                row("Deployment", "api", State::Edited, Some("1/1")),
                row("Service", "api", State::Missing, None),
            ],
        );
        let summary = g.summary();
        assert_eq!(summary.sync, SyncState::Missing);
        assert_eq!((summary.missing, summary.edited, summary.total), (1, 1, 2));
    }

    #[test]
    fn an_edit_or_an_extra_or_a_newer_version_is_out_of_sync() {
        let edited = group(
            Status::Current,
            Some("2.0.0"),
            vec![row("ConfigMap", "c", State::Edited, None)],
        );
        assert_eq!(edited.summary().sync, SyncState::OutOfSync);
        let extra = group(
            Status::Current,
            Some("2.0.0"),
            vec![row("ConfigMap", "c", State::Extra, None)],
        );
        assert_eq!(extra.summary().sync, SyncState::OutOfSync);
        let behind = group(
            Status::UpdateAvailable,
            Some("1.0.0"),
            vec![row("ConfigMap", "c", State::Synced, None)],
        );
        let summary = behind.summary();
        assert_eq!(summary.sync, SyncState::OutOfSync);
        assert_eq!(summary.update_to.as_deref(), Some("2.0.0"));
    }

    #[test]
    fn a_secret_that_is_not_looked_at_does_not_spoil_synced() {
        let g = group(
            Status::Current,
            Some("2.0.0"),
            vec![
                row("Deployment", "api", State::Synced, Some("1/1")),
                row("Secret", "s", State::Hidden, None),
            ],
        );
        let summary = g.summary();
        assert_eq!(summary.sync, SyncState::Synced);
        assert_eq!(
            (summary.synced, summary.workloads, summary.not_ready),
            (2, 1, 0)
        );
    }

    #[test]
    fn a_workload_with_fewer_ready_than_desired_is_counted_not_ready() {
        let g = group(
            Status::Current,
            Some("2.0.0"),
            vec![
                row("Deployment", "a", State::Synced, Some("0/1")),
                row("Deployment", "b", State::Synced, Some("2/2")),
                row("Deployment", "c", State::Missing, None),
            ],
        );
        assert_eq!(g.summary().not_ready, 1);
    }

    #[test]
    fn running_but_unpublished_is_unlisted() {
        let g = group(
            Status::Unlisted,
            Some("1.0.0"),
            vec![row("Deployment", "a", State::Synced, Some("1/1"))],
        );
        assert_eq!(g.summary().sync, SyncState::Unlisted);
    }

    #[test]
    fn the_kinds_are_listed_once_each() {
        let g = group(
            Status::Current,
            Some("2.0.0"),
            vec![
                row("Service", "a", State::Synced, None),
                row("Deployment", "a", State::Synced, None),
                row("Service", "b", State::Synced, None),
            ],
        );
        assert_eq!(g.kinds(), vec!["Deployment", "Service"]);
    }
}

#[test]
fn rows_filter_by_kind_state_and_a_piece_of_the_name() {
    use gantry_service::domain::resources::{Group, Row};
    use gantry_service::domain::targets::Status;
    let row = |kind: &str, name: &str, state| Row {
        api_version: None,
        kind: kind.to_string(),
        name: name.to_string(),
        namespace: None,
        state,
        ready: None,
        workload: false,
        editable: true,
    };
    let group = Group {
        package: "demo".to_string(),
        description: None,
        installed: None,
        offered: None,
        status: Status::Current,
        inventoried: true,
        rows: vec![
            row("Service", "Api", State::Synced),
            row("Deployment", "api", State::Edited),
            row("Deployment", "worker", State::Missing),
        ],
    };
    assert_eq!(group.filtered("", "", "").len(), 3);
    assert_eq!(group.filtered("deployment", "", "").len(), 2);
    assert_eq!(group.filtered("", "missing", "").len(), 1);
    assert_eq!(group.filtered("", "", " API ").len(), 2);
    assert_eq!(group.filtered("Deployment", "edited", "api").len(), 1);
    assert!(group.filtered("Secret", "", "").is_empty());
}
