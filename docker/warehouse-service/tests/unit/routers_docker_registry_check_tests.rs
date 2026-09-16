use crate::support;

use http::Method;
use warehouse_service::routers::docker::register_routes;

async fn app() -> (
    std::sync::Arc<dyn quench_http::endpoint::Endpoint>,
    std::sync::Arc<quench_http::di::Container>,
) {
    register_routes();
    let container = support::container_builder().build().await.unwrap();
    support::app(container).await
}

#[tokio::test]
async fn get_reports_ok_with_the_distribution_api_version_header() {
    let (app, container) = app().await;
    let req = support::req(Method::GET, "/v2/", &container);
    let resp = app.call(req).await;
    assert_eq!(resp.status(), http::StatusCode::OK);
    let (headers, _) = support::parts(resp).await;
    assert_eq!(
        headers.get("docker-distribution-api-version").unwrap(),
        "registry/2.0"
    );
}

#[tokio::test]
async fn head_reports_the_same_as_get() {
    let (app, container) = app().await;
    let req = support::req(Method::HEAD, "/v2/", &container);
    let resp = app.call(req).await;
    assert_eq!(resp.status(), http::StatusCode::OK);
    let (headers, _) = support::parts(resp).await;
    assert!(headers.contains_key("docker-distribution-api-version"));
}
