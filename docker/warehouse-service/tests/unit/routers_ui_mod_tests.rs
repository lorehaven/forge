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
    warehouse_service::routers::ui::register_routes();
    let container = support::container_builder()
        .provide(config(auth_enabled))
        .build()
        .await
        .unwrap();
    support::app(container).await
}

macro_rules! redirect_test {
    ($test_name:ident, $uri:expr, $unauthenticated_location:expr) => {
        #[tokio::test]
        async fn $test_name() {
            let (app, container) = app(true).await;
            let resp = app.call(support::req(Method::GET, $uri, &container)).await;
            assert!(resp.status().is_redirection());
            let (headers, _) = support::parts(resp).await;
            let location = support::location(&headers);
            assert!(location.contains($unauthenticated_location), "{location}");
        }
    };
}

redirect_test!(root_redirects_to_login_when_unauthenticated, "/ui", "login");
redirect_test!(
    root_slash_redirects_to_login_when_unauthenticated,
    "/ui/",
    "login"
);
redirect_test!(
    docker_root_redirects_to_login_when_unauthenticated,
    "/ui/docker",
    "login"
);
redirect_test!(
    docker_root_slash_redirects_to_login_when_unauthenticated,
    "/ui/docker/",
    "login"
);
redirect_test!(
    crates_root_redirects_to_login_when_unauthenticated,
    "/ui/crates",
    "login"
);
redirect_test!(
    crates_root_slash_redirects_to_login_when_unauthenticated,
    "/ui/crates/",
    "login"
);

#[tokio::test]
async fn root_redirects_home_when_authenticated() {
    let (app, container) = app(false).await;
    let resp = app.call(support::req(Method::GET, "/ui", &container)).await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    let (headers, _) = support::parts(resp).await;
    let location = support::location(&headers);
    assert!(location.contains("/ui/home"), "{location}");
}

#[tokio::test]
async fn docker_root_redirects_to_the_docker_catalog_when_authenticated() {
    let (app, container) = app(false).await;
    let resp = app
        .call(support::req(Method::GET, "/ui/docker", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::PERMANENT_REDIRECT);
    let (headers, _) = support::parts(resp).await;
    let location = support::location(&headers);
    assert!(location.contains("/ui/docker/catalog"), "{location}");
}

#[tokio::test]
async fn crates_root_redirects_to_the_crates_catalog_when_authenticated() {
    let (app, container) = app(false).await;
    let resp = app
        .call(support::req(Method::GET, "/ui/crates", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::PERMANENT_REDIRECT);
    let (headers, _) = support::parts(resp).await;
    let location = support::location(&headers);
    assert!(location.contains("/ui/crates/catalog"), "{location}");
}
