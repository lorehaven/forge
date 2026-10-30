//! The routes that start the browser client: the pages that open without a session, the pages behind one, and the files.

use gatehouse_service::test_support::service_auth_env_lock;
use gatehouse_service::tokens::VerificationTokens;
use quench_auth::domain::jwt::JwtConfig;
use quench_core::bytes::Bytes;
use quench_http::di::ContainerBuilder;
use quench_http::endpoint::Endpoint;
use quench_http::http::{HeaderMap, Method, StatusCode, Uri};
use quench_http::request::Request;
use std::sync::Arc;

type App = (Arc<dyn Endpoint>, Arc<quench_http::di::Container>);

async fn app(auth_enabled: bool) -> App {
    gatehouse_service::ui::register_routes();
    let mut config = JwtConfig::for_tests();
    config.auth_enabled = auth_enabled;
    let container = ContainerBuilder::new()
        .provide(config)
        .provide_arc(Arc::new(VerificationTokens::in_memory()))
        .build()
        .await
        .unwrap();
    (
        quench_starter::http::discover_and_mount("/"),
        Arc::new(container),
    )
}

async fn get(path: &str, app: &App) -> (StatusCode, HeaderMap, String) {
    let request = Request::new(
        Method::GET,
        path.parse::<Uri>().unwrap(),
        HeaderMap::new(),
        quench_http::body::InboundBody::from_bytes(Bytes::new()),
        app.1.clone(),
    );
    let response = app.0.call(request).await.into_inner();
    let (status, headers) = (response.status, response.headers);
    let body = response.body.collect_limited(usize::MAX).await.unwrap();
    (status, headers, String::from_utf8_lossy(&body).into_owned())
}

/// The page needs the built module (`web/app.wasm`): without it the service says so, which still shows that the visitor was
/// let through. `forge-web`'s own tests cover the page itself.
fn assert_served(path: &str, status: StatusCode, body: &str) {
    if status == StatusCode::OK {
        assert!(body.contains("/ui/assets/quench.js"), "{path}: {body}");
    } else {
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
        assert!(body.contains("not installed"), "{path}: {body}");
    }
}

const PUBLIC: [&str; 12] = [
    "/ui/login",
    "/ui/login/",
    "/ui/login?err=1&redirect=%2Fworkbench%2Fui%2Fhome",
    "/ui/login/mfa?pending=abc",
    "/ui/register",
    "/ui/register/",
    "/ui/forgot-password",
    "/ui/forgot-password/",
    "/ui/reset-password?token=abc",
    "/ui/resend-verification",
    "/ui/accept-invite?token=abc",
    "/ui/unsubscribe?token=abc",
];

const SIGNED_IN: [&str; 7] = [
    "/ui/home",
    "/ui/home/",
    "/ui/account",
    "/ui/account/mfa/enroll",
    "/ui/admin/users",
    "/ui/admin/users/",
    "/ui/admin/users/alice",
];

#[quench_test::test]
async fn the_pages_before_a_session_open_without_one() {
    let _guard = service_auth_env_lock().lock().await;
    let app = app(true).await;
    for path in PUBLIC {
        let (status, _, body) = get(path, &app).await;
        assert_served(path, status, &body);
    }
}

#[quench_test::test]
async fn a_link_that_stands_for_nothing_goes_to_the_login_error() {
    let _guard = service_auth_env_lock().lock().await;
    let app = app(true).await;
    let (status, headers, _) = get("/ui/confirm-email?token=nonsense", &app).await;
    assert_eq!(status, StatusCode::FOUND);
    let location = headers.get("location").unwrap().to_str().unwrap();
    assert!(location.ends_with("/ui/login?err=ui_login_confirm_email_invalid"));
}

#[quench_test::test]
async fn the_pages_that_need_the_token_refuse_without_it() {
    let _guard = service_auth_env_lock().lock().await;
    let app = app(true).await;
    for path in ["/ui/reset-password", "/ui/accept-invite", "/ui/login/mfa"] {
        let (status, _, _) = get(path, &app).await;
        assert!(status.is_client_error(), "{path}: {status}");
    }
}

#[quench_test::test]
async fn the_pages_behind_a_session_send_a_visitor_to_sign_in() {
    let _guard = service_auth_env_lock().lock().await;
    let app = app(true).await;
    for path in SIGNED_IN {
        let (status, headers, _) = get(path, &app).await;
        assert_eq!(status, StatusCode::FOUND, "{path}");
        let location = headers.get("location").unwrap().to_str().unwrap();
        assert!(location.contains("/ui/login"), "{path}: {location}");
    }
}

#[quench_test::test]
async fn the_pages_behind_a_session_start_the_client_when_there_is_one() {
    let _guard = service_auth_env_lock().lock().await;
    // With sign-in switched off the pages see the stand-in administrator.
    let app = app(false).await;
    for path in SIGNED_IN {
        let (status, _, body) = get(path, &app).await;
        assert_served(path, status, &body);
    }
}

#[quench_test::test]
async fn the_files_are_public_and_the_module_may_be_missing() {
    let _guard = service_auth_env_lock().lock().await;
    let app = app(true).await;
    let (status, _, body) = get("/ui/assets/app.css", &app).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains(".home-card"),
        "the stylesheet is the client's"
    );
    assert!(
        body.contains(".fui-topbar"),
        "and forge's frame comes with it"
    );
    let (status, _, body) = get("/ui/assets/quench.js", &app).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!body.is_empty());
    let (status, _, body) = get("/ui/assets/shell.js", &app).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("qlocale"));
    let (status, headers, _) = get("/ui/assets/favicon.png", &app).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.get("content-type").unwrap(), "image/png");
    let (status, _, _) = get("/ui/assets/app.wasm", &app).await;
    assert!(
        status == StatusCode::OK || status == StatusCode::NOT_FOUND,
        "{status}"
    );
}

#[quench_test::test]
async fn the_old_page_assets_are_gone() {
    let _guard = service_auth_env_lock().lock().await;
    let app = app(true).await;
    for path in ["/ui/assets/css/style.css", "/ui/assets/js/translations.js"] {
        let (status, _, _) = get(path, &app).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
    }
}
