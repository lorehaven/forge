use crate::support;
use gantry_service::domain::cluster::{FakeCluster, Workload};
use gantry_service::domain::executor::{ClusterExecutor, Executor};
use gantry_service::domain::operation::MemoryStore;
use gantry_service::domain::registry::MemoryRegistry;
use gantry_service::domain::service::Gantry;
use gantry_service::domain::settings::Settings;
use quench_auth::domain::jwt::Claims;
use quench_db::{Db, InMemoryDb};
use quench_http::http::{Method, StatusCode};
use std::sync::Arc;

struct Rig {
    app: Arc<dyn quench_http::endpoint::Endpoint>,
    container: Arc<quench_http::di::Container>,
    cluster: Arc<FakeCluster>,
}

async fn rig(auth_enabled: bool) -> Rig {
    gantry_service::routers::api::register_routes();
    let settings = Settings::inert();
    let cluster = Arc::new(FakeCluster::new());
    let registry = Arc::new(MemoryRegistry::default());
    registry.publish("media", "1.0.0", Some("media"));
    registry.publish("media", "1.1.0", Some("media"));
    registry.publish("forge", "2.0.0", Some("forge"));
    let executor: Arc<dyn Executor> = Arc::new(ClusterExecutor {
        cluster: cluster.clone(),
        settings: settings.clone(),
    });
    let gantry = Gantry::new(
        Arc::new(MemoryStore::new()),
        cluster.clone(),
        executor,
        registry,
        settings,
    );
    let container = quench_http::di::ContainerBuilder::new()
        .provide(support::jwt_config(auth_enabled))
        .provide(Db::InMemory(InMemoryDb::new()))
        .provide(gantry)
        .build()
        .await
        .unwrap();
    let (app, container) = support::app(container);
    Rig {
        app,
        container,
        cluster,
    }
}

fn token(scope: &str) -> Claims {
    Claims::for_audiences(
        "ana".to_string(),
        vec!["gantry".to_string()],
        scope.to_string(),
        None,
        3600,
    )
}

fn with(
    claims: Option<Claims>,
    mut request: quench_http::request::Request,
) -> quench_http::request::Request {
    if let Some(claims) = claims {
        request.extensions_mut().insert(claims);
    }
    request
}

fn running(version: &str) -> Vec<Workload> {
    vec![Workload {
        kind: "deployment".into(),
        name: "jellyfin".into(),
        namespace: "media".into(),
        package: "media".into(),
        version: Some(version.into()),
        desired: 1,
        ready: 1,
    }]
}

#[quench_test::test]
async fn targets_are_listed_with_what_runs_beside_what_is_published() {
    let rig = rig(false).await;
    rig.cluster.set_workloads(running("1.0.0"));

    let body = support::json_body(
        rig.app
            .call(support::req(Method::GET, "/api/v1/targets", &rig.container))
            .await,
    )
    .await;
    let media = body
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "media")
        .unwrap();
    assert_eq!(media["installed"], "1.0.0");
    assert_eq!(media["offered"], "1.1.0");
    assert_eq!(media["status"], "update_available");
    let forge = body
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "forge")
        .unwrap();
    assert_eq!(forge["status"], "not_installed");
}

#[quench_test::test]
async fn a_grant_scoped_to_one_package_shows_only_that_package() {
    let rig = rig(true).await;
    let request = with(
        Some(token("gantry:target:media:read")),
        support::req(Method::GET, "/api/v1/targets", &rig.container),
    );
    let body = support::json_body(rig.app.call(request).await).await;
    let names: Vec<&str> = body
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["media"]);

    let request = with(
        Some(token("gantry:target:media:read")),
        support::req(Method::GET, "/api/v1/targets/forge", &rig.container),
    );
    assert_eq!(rig.app.call(request).await.status(), StatusCode::FORBIDDEN);
}

#[quench_test::test]
async fn a_target_shows_its_versions_and_overrides() {
    let rig = rig(false).await;
    let body = support::json_body(
        rig.app
            .call(support::req(
                Method::GET,
                "/api/v1/targets/media",
                &rig.container,
            ))
            .await,
    )
    .await;
    assert_eq!(body["versions"].as_array().unwrap().len(), 2);
    assert_eq!(body["versions"][0]["version"], "1.1.0");

    let missing = rig
        .app
        .call(support::req(
            Method::GET,
            "/api/v1/targets/nope",
            &rig.container,
        ))
        .await;
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
}

#[quench_test::test]
async fn plan_then_confirm_queues_the_operation() {
    let rig = rig(false).await;
    rig.cluster.set_workloads(running("1.0.0"));

    let planned = rig
        .app
        .call(support::req_json(
            Method::POST,
            "/api/v1/targets/media/plan",
            &rig.container,
            "{}",
        ))
        .await;
    assert_eq!(planned.status(), StatusCode::CREATED);
    let plan = support::json_body(planned).await;
    assert_eq!(plan["action"], "upgrade");
    assert_eq!(plan["basis"], "1.0.0");
    let id = plan["id"].as_str().unwrap().to_string();

    let shown = support::json_body(
        rig.app
            .call(support::req(
                Method::GET,
                &format!("/api/v1/plans/{id}"),
                &rig.container,
            ))
            .await,
    )
    .await;
    assert_eq!(shown["plan"]["steps"][2]["step"], "install");

    let confirmed = rig
        .app
        .call(support::req_json(
            Method::POST,
            &format!("/api/v1/plans/{id}/confirm"),
            &rig.container,
            "{}",
        ))
        .await;
    assert_eq!(confirmed.status(), StatusCode::CREATED);
    assert_eq!(support::json_body(confirmed).await["state"], "queued");

    let again = rig
        .app
        .call(support::req_json(
            Method::POST,
            &format!("/api/v1/plans/{id}/confirm"),
            &rig.container,
            "{}",
        ))
        .await;
    assert_eq!(again.status(), StatusCode::CONFLICT);
}

#[quench_test::test]
async fn confirming_needs_the_permission_for_what_the_plan_does() {
    let rig = rig(true).await;
    rig.cluster.set_workloads(running("1.1.0"));

    // Going back to 1.0.0 is a rollback.
    let planned = rig
        .app
        .call(with(
            Some(token("gantry:target:media:read")),
            support::req_json(
                Method::POST,
                "/api/v1/targets/media/plan",
                &rig.container,
                r#"{"version":"1.0.0"}"#,
            ),
        ))
        .await;
    assert_eq!(planned.status(), StatusCode::CREATED);
    let id = support::json_body(planned).await["id"]
        .as_str()
        .unwrap()
        .to_string();

    let confirm = |scope: &str| {
        with(
            Some(token(scope)),
            support::req_json(
                Method::POST,
                &format!("/api/v1/plans/{id}/confirm"),
                &rig.container,
                "{}",
            ),
        )
    };
    // `deploy` does not cover going backwards...
    assert_eq!(
        rig.app
            .call(confirm("gantry:target:media:deploy"))
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    // ...`rollback` on that one package does.
    assert_eq!(
        rig.app
            .call(confirm("gantry:target:media:rollback"))
            .await
            .status(),
        StatusCode::CREATED
    );
}

#[quench_test::test]
async fn planning_a_missing_package_is_404_and_a_missing_version_is_422() {
    let rig = rig(false).await;
    let missing = rig
        .app
        .call(support::req_json(
            Method::POST,
            "/api/v1/targets/nope/plan",
            &rig.container,
            "{}",
        ))
        .await;
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);

    let bad = rig
        .app
        .call(support::req_json(
            Method::POST,
            "/api/v1/targets/media/plan",
            &rig.container,
            r#"{"version":"9.9.9"}"#,
        ))
        .await;
    assert_eq!(bad.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[quench_test::test]
async fn a_plan_is_read_with_its_steps_in_words_and_whether_the_caller_may_run_it() {
    use quench_serde::Deserialize;
    let rig = rig(true).await;
    rig.cluster.set_workloads(running("1.0.0"));
    let mut request = support::req_json(
        Method::POST,
        "/api/v1/targets/media/plan",
        &rig.container,
        "{}",
    );
    request.extensions_mut().insert(token("gantry:read"));
    let id = support::json_body(rig.app.call(request).await).await["id"]
        .as_str()
        .unwrap()
        .to_string();

    for (scope, may) in [("gantry:read", false), ("gantry:read gantry:deploy", true)] {
        let mut request = support::req(Method::GET, &format!("/api/v1/plans/{id}"), &rig.container);
        request.extensions_mut().insert(token(scope));
        let shown = support::json_body(rig.app.call(request).await).await;
        assert_eq!(shown["can_confirm"], may, "{scope}");
        assert_eq!(shown["step_lines"][0], "download media 1.1.0");
        // The browser client reads it as it is.
        let plan = gantry_core::StoredPlan::deserialize(&shown).unwrap();
        assert_eq!(plan.can_confirm, may);
        assert_eq!(plan.action, "upgrade");
        assert!(!plan.step_lines.is_empty());
    }
}
