use crate::support;
use gantry_service::domain::cluster::FakeCluster;
use gantry_service::domain::executor::{ClusterExecutor, Executor};
use gantry_service::domain::operation::MemoryStore;
use gantry_service::domain::registry::MemoryRegistry;
use gantry_service::domain::service::Gantry;
use gantry_service::domain::settings::Settings;
use http::{Method, StatusCode};
use quench_auth::domain::jwt::Claims;
use quench_db::{Db, InMemoryDb};
use std::sync::Arc;

async fn app(
    auth_enabled: bool,
    allowed: &[&str],
) -> (
    Arc<dyn quench_http::endpoint::Endpoint>,
    Arc<quench_http::di::Container>,
) {
    gantry_service::routers::api::register_routes();
    let mut settings = Settings::inert();
    settings.allowed_namespaces = allowed.iter().map(|s| (*s).to_string()).collect();
    let cluster = Arc::new(FakeCluster::new());
    let executor: Arc<dyn Executor> = Arc::new(ClusterExecutor {
        cluster: cluster.clone(),
        settings: settings.clone(),
    });
    let gantry = Gantry::new(
        Arc::new(MemoryStore::new()),
        cluster,
        executor,
        Arc::new(MemoryRegistry::default()),
        settings,
    );
    let container = quench_http::di::ContainerBuilder::new()
        .provide(support::jwt_config(auth_enabled))
        .provide(Db::InMemory(InMemoryDb::new()))
        .provide(gantry)
        .build()
        .await
        .unwrap();
    support::app(container).await
}

fn post(
    path: &str,
    container: &Arc<quench_http::di::Container>,
    json: &str,
) -> quench_http::request::Request {
    support::req_json(Method::POST, path, container, json)
}

const PLAN: &str = r#"{"title":"Scale sage down","plan":{"steps":[{"step":"scale","namespace":"forge","kind":"deployment","name":"sage","replicas":0}]}}"#;

#[tokio::test]
async fn an_operation_is_recorded_listed_and_read_back() {
    let (app, container) = app(false, &[]).await;

    let created = app.call(post("/api/v1/operations", &container, PLAN)).await;
    assert_eq!(created.status(), StatusCode::CREATED);
    let body = support::json_body(created).await;
    assert_eq!(body["state"], "queued");
    assert_eq!(body["requested_by"], "dev");
    assert_eq!(body["kind"], "custom");
    let id = body["id"].as_str().unwrap().to_string();

    let listed = support::json_body(
        app.call(support::req(Method::GET, "/api/v1/operations", &container))
            .await,
    )
    .await;
    assert_eq!(listed.as_array().unwrap().len(), 1);

    let read = support::json_body(
        app.call(support::req(
            Method::GET,
            &format!("/api/v1/operations/{id}"),
            &container,
        ))
        .await,
    )
    .await;
    assert_eq!(read["title"], "Scale sage down");
    assert_eq!(read["plan"]["steps"][0]["step"], "scale");
}

#[tokio::test]
async fn a_plan_that_fails_validation_is_422_and_nothing_is_recorded() {
    let (app, container) = app(false, &["scratch"]).await;
    let response = app.call(post("/api/v1/operations", &container, PLAN)).await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert!(support::body_text(response).await.contains("allow-list"));

    let listed = support::json_body(
        app.call(support::req(Method::GET, "/api/v1/operations", &container))
            .await,
    )
    .await;
    assert!(listed.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn an_unknown_operation_is_404() {
    let (app, container) = app(false, &[]).await;
    let response = app
        .call(support::req(
            Method::GET,
            "/api/v1/operations/nope",
            &container,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_queued_operation_can_be_cancelled() {
    let (app, container) = app(false, &[]).await;
    let id = support::json_body(app.call(post("/api/v1/operations", &container, PLAN)).await).await
        ["id"]
        .as_str()
        .unwrap()
        .to_string();
    let cancelled = support::json_body(
        app.call(post(
            &format!("/api/v1/operations/{id}/cancel"),
            &container,
            "{}",
        ))
        .await,
    )
    .await;
    assert_eq!(cancelled["state"], "cancelled");
}

#[tokio::test]
async fn reading_needs_read_and_submitting_needs_deploy() {
    let (app, container) = app(true, &[]).await;
    let token = |scope: &str| {
        Claims::for_audiences(
            "ana".to_string(),
            vec!["gantry".to_string()],
            scope.to_string(),
            None,
            3600,
        )
    };

    let mut request = post("/api/v1/operations", &container, PLAN);
    request.extensions_mut().insert(token("gantry:read"));
    assert_eq!(app.call(request).await.status(), StatusCode::FORBIDDEN);

    let mut request = post("/api/v1/operations", &container, PLAN);
    request.extensions_mut().insert(token("gantry:deploy"));
    let response = app.call(request).await;
    assert_eq!(response.status(), StatusCode::CREATED);
    assert_eq!(support::json_body(response).await["requested_by"], "ana");

    let mut request = support::req(Method::GET, "/api/v1/operations", &container);
    request.extensions_mut().insert(token("gantry:deploy"));
    assert_eq!(app.call(request).await.status(), StatusCode::FORBIDDEN);

    let mut request = support::req(Method::GET, "/api/v1/operations", &container);
    request.extensions_mut().insert(token("gantry:read"));
    assert_eq!(app.call(request).await.status(), StatusCode::OK);
}
