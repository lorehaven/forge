//! The routes that start the browser client: the page behind the session check, and the files it loads.

use quench_auth::domain::jwt::JwtConfig;
use quench_core::bytes::Bytes;
use quench_http::di::ContainerBuilder;
use quench_http::endpoint::Endpoint;
use quench_http::http::{HeaderMap, Method, StatusCode, Uri};
use quench_http::request::Request;
use sage_service::routers::ui;
use std::sync::Arc;

async fn app(auth_enabled: bool) -> (Arc<dyn Endpoint>, Arc<quench_http::di::Container>) {
    ui::register_routes();
    let mut config = JwtConfig::for_tests();
    config.auth_enabled = auth_enabled;
    let container = ContainerBuilder::new()
        .provide(config)
        .build()
        .await
        .unwrap();
    (
        quench_starter::http::discover_and_mount("/"),
        Arc::new(container),
    )
}

async fn get(
    path: &str,
    app: &(Arc<dyn Endpoint>, Arc<quench_http::di::Container>),
) -> (StatusCode, String) {
    let request = Request::new(
        Method::GET,
        path.parse::<Uri>().unwrap(),
        HeaderMap::new(),
        quench_http::body::InboundBody::from_bytes(Bytes::new()),
        app.1.clone(),
    );
    let response = app.0.call(request).await.into_inner();
    let status = response.status;
    let body = response.body.collect_limited(usize::MAX).await.unwrap();
    (status, String::from_utf8_lossy(&body).into_owned())
}

/// Every page the client has, as an address (the conversation and the project are in the query).
const PAGES: [&str; 5] = [
    "/ui/home",
    "/ui/home/",
    "/ui/home?conversation_id=c1&project_id=p1",
    "/ui/home?project_id=p1",
    "/ui/initializing",
];

/// The page needs the built module (`web/app.wasm`): without it the service says so, which still shows that the visitor got
/// past the session check. `forge-web`'s own tests cover the page itself.
#[quench_test::test]
async fn the_pages_start_the_client_for_a_signed_in_visitor() {
    let app = app(false).await;
    for path in PAGES {
        let (status, body) = get(path, &app).await;
        if status == StatusCode::OK {
            assert!(body.contains("/ui/assets/quench.js"), "{path}: {body}");
        } else {
            assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
            assert!(body.contains("not installed"), "{path}: {body}");
        }
    }
}

#[quench_test::test]
async fn a_visitor_with_no_session_is_sent_to_sign_in() {
    let app = app(true).await;
    for path in PAGES {
        let (status, _) = get(path, &app).await;
        assert!(status.is_redirection(), "{path}: {status}");
    }
}

#[quench_test::test]
async fn the_files_are_public_and_the_module_may_be_missing() {
    let app = app(true).await;
    let (status, body) = get("/ui/assets/app.css", &app).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains(".sg-chat"), "{body}");
    let (status, body) = get("/ui/assets/quench.js", &app).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!body.is_empty());
    let (status, _) = get("/ui/assets/app.wasm", &app).await;
    assert!(
        status == StatusCode::OK || status == StatusCode::NOT_FOUND,
        "{status}"
    );
}

#[quench_test::test]
async fn the_root_goes_to_the_front_page_or_to_sign_in() {
    let app_in = app(false).await;
    for path in ["/ui", "/ui/"] {
        let (status, _) = get(path, &app_in).await;
        assert!(status.is_redirection(), "{path}: {status}");
    }
    let app_out = app(true).await;
    let (status, _) = get("/ui", &app_out).await;
    assert!(status.is_redirection(), "{status}");
}
