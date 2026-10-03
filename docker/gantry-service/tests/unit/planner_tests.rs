use gantry_service::domain::cluster::{FakeCluster, Workload};
use gantry_service::domain::executor::{ClusterExecutor, Executor};
use gantry_service::domain::operation::{MemoryStore, State};
use gantry_service::domain::planner::{self, Action, PlanError};
use gantry_service::domain::registry::{self, DirRegistry, MemoryRegistry, Registry, compare};
use gantry_service::domain::service::Gantry;
use gantry_service::domain::settings::Settings;
use gantry_service::domain::steps::Step;
use gantry_service::domain::targets::Status;
use std::cmp::Ordering;
use std::sync::Arc;

struct Rig {
    cluster: Arc<FakeCluster>,
    registry: Arc<MemoryRegistry>,
    gantry: Gantry,
}

fn rig_with(settings: Settings) -> Rig {
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

fn rig() -> Rig {
    rig_with(Settings::inert())
}

fn workload(package: &str, name: &str, namespace: &str, version: Option<&str>) -> Workload {
    Workload {
        kind: "deployment".into(),
        name: name.into(),
        namespace: namespace.into(),
        package: package.into(),
        version: version.map(str::to_string),
        desired: 1,
        ready: 1,
    }
}

// ---------------------------------------------------------------- versions

#[test]
fn versions_order_by_semver_then_by_build_metadata_text() {
    assert_eq!(compare("1.0.0", "1.0.1"), Ordering::Less);
    assert_eq!(compare("1.10.0", "1.9.0"), Ordering::Greater);
    assert_eq!(compare("1.0.0-rc.1", "1.0.0"), Ordering::Less);
    // Semver ignores build metadata; the registry orders it as text, timestamp first.
    assert_eq!(
        compare("0.4.0+20261001.aaa", "0.4.0+20261002.bbb"),
        Ordering::Less
    );
    assert_eq!(compare("0.4.0+x", "0.4.0+x"), Ordering::Equal);
}

#[tokio::test]
async fn the_newest_version_skips_the_yanked_one() {
    let registry = MemoryRegistry::default();
    registry.publish("forge", "1.0.0", Some("forge"));
    registry.publish("forge", "1.1.0", Some("forge"));
    registry.yank("forge", "1.1.0");

    let versions = registry.versions("forge").await.unwrap();
    assert_eq!(
        versions[0].version, "1.1.0",
        "newest first, yanked included"
    );
    assert_eq!(registry::newest(&versions).unwrap().version, "1.0.0");
    assert_eq!(registry.catalog().await.unwrap()[0].version, "1.0.0");
}

#[tokio::test]
async fn a_directory_of_rivet_files_is_a_registry() {
    let dir = tempfile::tempdir().unwrap();
    for version in ["0.1.0", "0.2.0"] {
        let manifest = rivet_package::Manifest::new("demo", version);
        let mut builder = rivet_package::PackageBuilder::new(manifest);
        builder
            .add_file("overlay.yaml", b"resources: []\n".to_vec())
            .unwrap();
        std::fs::write(
            dir.path().join(format!("demo-{version}.rivet")),
            builder.build().unwrap(),
        )
        .unwrap();
    }
    std::fs::write(dir.path().join("junk.rivet"), b"not a package").unwrap();
    std::fs::write(dir.path().join("notes.txt"), b"ignored").unwrap();

    let registry = DirRegistry {
        dir: dir.path().to_path_buf(),
    };
    let catalog = registry.catalog().await.unwrap();
    assert_eq!(catalog.len(), 1);
    assert_eq!(
        (catalog[0].name.as_str(), catalog[0].version.as_str()),
        ("demo", "0.2.0")
    );
    assert_eq!(catalog[0].sha256.len(), 64);
    assert_eq!(registry.versions("demo").await.unwrap().len(), 2);
}

// ---------------------------------------------------------------- targets

#[tokio::test]
async fn each_target_says_how_what_runs_compares_to_what_is_published() {
    let rig = rig();
    for (name, version) in [
        ("fresh", "1.0.0"),
        ("current", "1.0.0"),
        ("behind", "1.1.0"),
        ("ahead", "1.0.0"),
        ("split", "1.0.0"),
    ] {
        rig.registry.publish(name, version, Some("ns"));
    }
    rig.cluster.set_workloads(vec![
        workload("current", "a", "ns", Some("1.0.0")),
        workload("behind", "b", "ns", Some("1.0.0")),
        workload("ahead", "c", "ns", Some("2.0.0")),
        workload("split", "d", "ns", Some("1.0.0")),
        workload("split", "e", "ns", Some("0.9.0")),
        workload("handmade", "f", "ns", Some("1.0.0")),
    ]);

    let targets = planner::all(&rig.gantry).await.unwrap();
    let status = |name: &str| targets.iter().find(|t| t.name == name).unwrap().status;
    assert_eq!(status("fresh"), Status::NotInstalled);
    assert_eq!(status("current"), Status::Current);
    assert_eq!(status("behind"), Status::UpdateAvailable);
    assert_eq!(status("ahead"), Status::Ahead);
    assert_eq!(status("split"), Status::Mixed);
    assert_eq!(status("handmade"), Status::Unlisted);

    let behind = targets.iter().find(|t| t.name == "behind").unwrap();
    assert_eq!(behind.installed.as_deref(), Some("1.0.0"));
    assert_eq!(behind.offered.as_deref(), Some("1.1.0"));
    assert_eq!(behind.units.len(), 1);
}

// ---------------------------------------------------------------- plans

#[tokio::test]
async fn installing_a_new_package_is_pull_check_install_into_its_namespace() {
    let rig = rig();
    rig.registry.publish("media", "1.0.0", Some("media"));

    let (stored, action) = planner::plan(&rig.gantry, "media", None, "ana")
        .await
        .unwrap();
    assert_eq!(action, Action::Install);
    assert_eq!(stored.basis, None);
    assert_eq!(stored.version.as_deref(), Some("1.0.0"));

    let steps = &stored.plan.steps;
    assert!(
        matches!(&steps[0], Step::Pull { package, version } if package == "media" && version == "1.0.0")
    );
    assert!(
        matches!(
            &steps[1],
            Step::Check {
                values_secret: None,
                ..
            }
        ),
        "no values Secret: the package carries its own values and its encrypted secrets"
    );
    let Step::Install {
        namespace,
        timeout_secs,
        ..
    } = &steps[2]
    else {
        panic!("third step should install")
    };
    assert_eq!(namespace, "media");
    assert!(timeout_secs.is_some());
    assert!(stored.plan.summary[0].contains("Install media 1.0.0 into media"));
}

#[tokio::test]
async fn an_upgrade_names_both_versions_and_remembers_what_it_was_made_against() {
    let rig = rig();
    rig.registry.publish("media", "1.0.0", Some("media"));
    rig.registry.publish("media", "1.1.0", Some("media"));
    rig.cluster
        .set_workloads(vec![workload("media", "jellyfin", "media", Some("1.0.0"))]);

    let (stored, action) = planner::plan(&rig.gantry, "media", None, "ana")
        .await
        .unwrap();
    assert_eq!(action, Action::Upgrade);
    assert_eq!(stored.basis.as_deref(), Some("1.0.0"));
    assert!(
        stored.plan.summary[0].contains("from 1.0.0 to 1.1.0"),
        "{:?}",
        stored.plan.summary
    );
}

#[tokio::test]
async fn going_to_an_older_version_is_a_rollback_and_needs_that_permission() {
    let rig = rig();
    rig.registry.publish("media", "1.0.0", Some("media"));
    rig.registry.publish("media", "1.1.0", Some("media"));
    rig.cluster
        .set_workloads(vec![workload("media", "jellyfin", "media", Some("1.1.0"))]);

    let (_, action) = planner::plan(&rig.gantry, "media", Some("1.0.0"), "ana")
        .await
        .unwrap();
    assert_eq!(action, Action::Downgrade);
    assert_eq!(action.permission(), "rollback");
    assert_eq!(Action::Upgrade.permission(), "deploy");
    assert_eq!(Action::parse("downgrade"), Some(Action::Downgrade));
}

#[tokio::test]
async fn a_yanked_version_may_stay_where_it_runs_but_not_go_anywhere_new() {
    let rig = rig();
    rig.registry.publish("media", "1.0.0", Some("media"));
    rig.registry.publish("media", "1.1.0", Some("media"));
    rig.registry.yank("media", "1.1.0");

    let error = planner::plan(&rig.gantry, "media", Some("1.1.0"), "ana")
        .await
        .unwrap_err();
    assert!(
        matches!(&error, PlanError::Refused(m) if m.contains("yanked")),
        "{error}"
    );

    // Newest by default skips it...
    let (stored, _) = planner::plan(&rig.gantry, "media", None, "ana")
        .await
        .unwrap();
    assert_eq!(stored.version.as_deref(), Some("1.0.0"));

    // ...but re-applying it where it already runs is fine.
    rig.cluster
        .set_workloads(vec![workload("media", "jellyfin", "media", Some("1.1.0"))]);
    let (stored, action) = planner::plan(&rig.gantry, "media", Some("1.1.0"), "ana")
        .await
        .unwrap();
    assert_eq!(
        (stored.version.as_deref(), action),
        (Some("1.1.0"), Action::Reinstall)
    );
}

#[tokio::test]
async fn unknown_packages_and_versions_are_refused() {
    let rig = rig();
    rig.registry.publish("media", "1.0.0", Some("media"));
    assert!(matches!(
        planner::plan(&rig.gantry, "nope", None, "ana")
            .await
            .unwrap_err(),
        PlanError::UnknownTarget(_)
    ));
    assert!(matches!(
        planner::plan(&rig.gantry, "media", Some("9.9.9"), "ana")
            .await
            .unwrap_err(),
        PlanError::Refused(_)
    ));
}

#[tokio::test]
async fn a_plan_that_replaces_gantry_itself_applies_it_last_alone_and_guarded() {
    let mut settings = Settings::inert();
    settings.self_deployment = Some(("forge".into(), "gantry".into()));
    let rig = rig_with(settings);
    rig.registry.publish("forge", "2.0.0", Some("forge"));
    rig.cluster.set_workloads(vec![
        workload("forge", "gatehouse", "forge", Some("1.0.0")),
        workload("forge", "gantry", "forge", Some("1.0.0")),
    ]);

    let (stored, _) = planner::plan(&rig.gantry, "forge", None, "ana")
        .await
        .unwrap();
    let kinds: Vec<&str> = stored
        .plan
        .steps
        .iter()
        .map(|s| match s {
            Step::Pull { .. } => "pull",
            Step::Check { .. } => "check",
            Step::Install { .. } => "install",
            Step::Rollout { .. } => "rollout",
            _ => "other",
        })
        .collect();
    assert_eq!(kinds, ["pull", "check", "install", "install", "rollout"]);

    // Everything but Gantry first, waiting for each rollout...
    let Step::Install {
        except,
        targets,
        no_wait,
        ..
    } = &stored.plan.steps[2]
    else {
        panic!()
    };
    assert_eq!(except, &["deployment/gantry"]);
    assert!(targets.is_empty() && !no_wait);
    // ...then Gantry alone, not waited on by riveter, and the guarded rollout owns the outcome.
    let Step::Install {
        except,
        targets,
        no_wait,
        ..
    } = &stored.plan.steps[3]
    else {
        panic!()
    };
    assert!(except.is_empty());
    assert_eq!(targets, &["deployment/gantry"]);
    assert!(*no_wait);
    let Some(Step::Rollout {
        name,
        rollback_on_failure,
        ..
    }) = stored.plan.steps.last()
    else {
        panic!()
    };
    assert_eq!(name, "gantry");
    assert!(rollback_on_failure);
    assert!(
        stored
            .plan
            .summary
            .iter()
            .any(|l| l.contains("Includes Gantry itself"))
    );
}

#[tokio::test]
async fn a_plan_that_does_not_touch_gantry_has_no_special_steps() {
    let mut settings = Settings::inert();
    settings.self_deployment = Some(("forge".into(), "gantry".into()));
    let rig = rig_with(settings);
    rig.registry.publish("media", "2.0.0", Some("media"));
    rig.cluster
        .set_workloads(vec![workload("media", "jellyfin", "media", Some("1.0.0"))]);
    let (stored, _) = planner::plan(&rig.gantry, "media", None, "ana")
        .await
        .unwrap();
    assert_eq!(stored.plan.steps.len(), 3);
    let Step::Install {
        except, no_wait, ..
    } = &stored.plan.steps[2]
    else {
        panic!()
    };
    assert!(except.is_empty() && !no_wait);
}

#[tokio::test]
async fn a_package_with_no_namespace_and_nothing_running_cannot_be_planned() {
    let rig = rig();
    rig.registry.publish("odd", "1.0.0", None);
    let error = planner::plan(&rig.gantry, "odd", None, "ana")
        .await
        .unwrap_err();
    assert!(
        matches!(&error, PlanError::Refused(m) if m.contains("namespace")),
        "{error}"
    );
}

#[tokio::test]
async fn a_plan_outside_the_allow_list_is_refused_up_front() {
    let mut settings = Settings::inert();
    settings.allowed_namespaces = vec!["scratch".into()];
    let rig = rig_with(settings);
    rig.registry.publish("media", "1.0.0", Some("media"));
    let error = planner::plan(&rig.gantry, "media", None, "ana")
        .await
        .unwrap_err();
    assert!(
        matches!(&error, PlanError::Refused(m) if m.contains("allow-list")),
        "{error}"
    );
}

// ---------------------------------------------------------------- confirming

#[tokio::test]
async fn confirming_turns_the_plan_into_one_queued_operation_and_only_once() {
    let rig = rig();
    rig.registry.publish("media", "1.0.0", Some("media"));
    let (stored, _) = planner::plan(&rig.gantry, "media", None, "ana")
        .await
        .unwrap();

    let operation = planner::confirm(&rig.gantry, &stored.id, "bob")
        .await
        .unwrap();
    assert_eq!(operation.state, State::Queued);
    assert_eq!(operation.requested_by, "bob");
    assert_eq!(operation.kind, "install");
    assert_eq!(operation.plan, stored.plan);

    let again = planner::confirm(&rig.gantry, &stored.id, "bob")
        .await
        .unwrap_err();
    assert!(matches!(again, PlanError::Stale(_)), "{again}");
    assert_eq!(rig.gantry.store.list(10).await.unwrap().len(), 1);
    assert_eq!(
        rig.gantry
            .store
            .plan(&stored.id)
            .await
            .unwrap()
            .unwrap()
            .operation_id
            .as_deref(),
        Some(operation.id.as_str())
    );
}

#[tokio::test]
async fn a_plan_made_against_an_older_cluster_is_refused() {
    let rig = rig();
    rig.registry.publish("media", "1.0.0", Some("media"));
    rig.registry.publish("media", "1.1.0", Some("media"));
    rig.cluster
        .set_workloads(vec![workload("media", "jellyfin", "media", Some("1.0.0"))]);
    let (stored, _) = planner::plan(&rig.gantry, "media", None, "ana")
        .await
        .unwrap();

    // Someone else upgraded it in the meantime.
    rig.cluster
        .set_workloads(vec![workload("media", "jellyfin", "media", Some("1.1.0"))]);

    let error = planner::confirm(&rig.gantry, &stored.id, "ana")
        .await
        .unwrap_err();
    assert!(
        matches!(&error, PlanError::Stale(m) if m.contains("plan it again")),
        "{error}"
    );
    assert!(rig.gantry.store.list(10).await.unwrap().is_empty());
    assert!(
        rig.gantry
            .store
            .plan(&stored.id)
            .await
            .unwrap()
            .unwrap()
            .operation_id
            .is_none(),
        "a refused confirm must not use the plan up"
    );
}
