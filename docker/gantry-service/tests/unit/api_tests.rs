use crate::support;
use http::{Method, StatusCode};
use quench_auth::domain::jwt::Claims;
use quench_db::{Db, InMemoryDb};

async fn app(
    auth_enabled: bool,
) -> (
    std::sync::Arc<dyn quench_http::endpoint::Endpoint>,
    std::sync::Arc<quench_http::di::Container>,
) {
    gantry_service::routers::api::register_routes();
    let container = support::basic_container(
        support::jwt_config(auth_enabled),
        Db::InMemory(InMemoryDb::new()),
    )
    .await;
    support::app(container).await
}

#[tokio::test]
async fn info_names_the_service_and_reports_everything_allowed_when_auth_is_off() {
    let (app, container) = app(false).await;
    let resp = app
        .call(support::req(Method::GET, "/api/v1/info", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body = support::json_body(resp).await;
    assert_eq!(body["service"], "gantry");
    assert_eq!(body["actor"], "dev");
    assert_eq!(body["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(
        body["can"],
        serde_json::json!(["read", "deploy", "scale", "activate", "rollback"])
    );
}

#[tokio::test]
async fn info_reports_only_what_the_token_grants() {
    let (app, container) = app(true).await;
    let claims = Claims::for_audiences(
        "ci".to_string(),
        vec!["gantry".to_string()],
        "gantry:read gantry:scale".to_string(),
        None,
        3600,
    );
    let mut request = support::req(Method::GET, "/api/v1/info", &container);
    request.extensions_mut().insert(claims);

    let body = support::json_body(app.call(request).await).await;
    assert_eq!(body["actor"], "ci");
    assert_eq!(body["can"], serde_json::json!(["read", "scale"]));
}
