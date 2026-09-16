//! Unit tests for `routers/ui/pages/projects.rs`.

use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode, Uri};
use http_body_util::BodyExt;
use quench_auth::domain::jwt::JwtConfig;
use quench_db::InMemoryDb;
use quench_db::prelude::{Crud, Db};
use quench_http::di::ContainerBuilder;
use quench_http::endpoint::Endpoint;
use quench_http::request::Request;
use sage_service::domain::models::Project;
use sage_service::routers::ui::pages::projects;
use std::sync::Arc;

async fn app(
    jwt_config: JwtConfig,
    db: Db,
) -> (Arc<dyn Endpoint>, Arc<quench_http::di::Container>) {
    projects::register_routes();
    let container = ContainerBuilder::new()
        .provide(jwt_config)
        .provide(db)
        .build()
        .await
        .unwrap();
    (
        quench_starter::http::discover_and_mount("/"),
        Arc::new(container),
    )
}

fn req(method: Method, path: &str, container: &Arc<quench_http::di::Container>) -> Request {
    Request::new(
        method,
        path.parse::<Uri>().unwrap(),
        HeaderMap::new(),
        quench_http::body::InboundBody::from_bytes(Bytes::new()),
        container.clone(),
    )
}

fn form_req(
    path: &str,
    pairs: &[(&str, &str)],
    container: &Arc<quench_http::di::Container>,
) -> Request {
    let encoded = serde_urlencoded::to_string(pairs).unwrap();
    let mut headers = HeaderMap::new();
    headers.insert(
        "content-type",
        "application/x-www-form-urlencoded".parse().unwrap(),
    );
    Request::new(
        Method::POST,
        path.parse::<Uri>().unwrap(),
        headers,
        quench_http::body::InboundBody::from_bytes(Bytes::from(encoded)),
        container.clone(),
    )
}

async fn body_text(resp: quench_http::response::Response) -> String {
    let collected = resp.into_hyper().into_body().collect().await.expect("body");
    String::from_utf8_lossy(&collected.to_bytes()).into_owned()
}

#[tokio::test]
async fn new_modal_renders_the_create_form() {
    let (app, container) = app(JwtConfig::for_tests(), Db::InMemory(InMemoryDb::new())).await;
    let resp = app
        .call(req(Method::GET, "/ui/projects/new-modal", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);

    let html = body_text(resp).await;
    assert!(html.contains("new-project-modal"));
    assert!(html.contains("project-name"));
}

#[tokio::test]
async fn create_project_is_unauthorized_without_a_claim_when_auth_is_required() {
    let mut config = JwtConfig::for_tests();
    config.auth_enabled = true;
    let (app, container) = app(config, Db::InMemory(InMemoryDb::new())).await;

    let resp = app
        .call(form_req(
            "/ui/projects/create",
            &[("name", "My Project")],
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn create_project_stores_the_project_owned_by_the_admin_bypass_user() {
    let db = Db::InMemory(InMemoryDb::new());
    let (app, container) = app(JwtConfig::for_tests(), db.clone()).await;

    let resp = app
        .call(form_req(
            "/ui/projects/create",
            &[("name", "My Project")],
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let redirect = resp
        .into_hyper()
        .headers()
        .get("hx-redirect")
        .expect("HX-Redirect header")
        .to_str()
        .unwrap()
        .to_string();
    assert!(redirect.contains("/ui/home?project_id="));

    let projects = db.repository::<Project>().list().await.unwrap();
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0].name, "My Project");
    assert_eq!(projects[0].owner, "admin");
}
