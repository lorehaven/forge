use crate::support;
use http::{Method, StatusCode};
use quench_auth::domain::jwt::JwtConfig;

fn config(auth_enabled: bool) -> JwtConfig {
    let mut config = JwtConfig::for_tests();
    config.auth_enabled = auth_enabled;
    config
}

async fn app(
    auth_enabled: bool,
) -> (
    std::sync::Arc<dyn quench_http::endpoint::Endpoint>,
    std::sync::Arc<quench_http::di::Container>,
) {
    warehouse_service::routers::ui::pages::docker::tags::register_routes();
    let container = support::container_builder()
        .provide(config(auth_enabled))
        .build()
        .await
        .unwrap();
    support::app(container).await
}

#[tokio::test]
async fn redirects_to_login_when_auth_is_enabled_and_there_is_no_session() {
    let (app, container) = app(true).await;
    let resp = app
        .call(support::req(
            Method::GET,
            "/ui/docker/tags/my-repo",
            &container,
        ))
        .await;
    assert!(resp.status().is_redirection());
}

#[tokio::test]
async fn redirects_to_the_catalog_filtered_by_repository_when_authenticated() {
    let (app, container) = app(false).await;
    let resp = app
        .call(support::req(
            Method::GET,
            "/ui/docker/tags/my-repo",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::PERMANENT_REDIRECT);
    let (headers, _) = support::parts(resp).await;
    let location = support::location(&headers);
    assert!(
        location.contains("/ui/docker/catalog?repo=my-repo"),
        "{location}"
    );
}
