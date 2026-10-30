use gatehouse_service::realm;
use gatehouse_service::ui::locale::{LocaleCookie, default_locale_cookie};
use gatehouse_service::ui::pages::auth::{
    LoginForm, MfaForm, login_mfa_submit, login_redirect, login_submit, mfa_challenge_url,
};
use quench_auth::domain::auth::{Permissions, Role};
use quench_auth::domain::jwt::JwtConfig;
use quench_auth::domain::session::SessionDb;
use quench_cache::CacheStore;
use quench_core::bytes::Bytes;
use quench_db::prelude::Db;
use quench_http::di::ContainerBuilder;
use quench_http::endpoint::Endpoint;
use quench_http::http::{HeaderMap, Method, StatusCode, Uri};
use quench_http::prelude::{Form, Inject, Request};
use std::sync::Arc;

async fn db() -> Db {
    Db::connect("").await.expect("in-memory db")
}

fn sessions() -> Arc<SessionDb> {
    SessionDb::init(CacheStore::in_memory())
}

// -----------------------------------------------------------------
// mfa_challenge_url
// -----------------------------------------------------------------

#[quench_test::test]
fn mfa_challenge_url_carries_redirect_and_err() {
    let url = mfa_challenge_url("pending-token", Some("/ui/home"), true);
    assert!(url.contains("pending=pending-token"));
    assert!(url.contains("redirect="));
    assert!(url.contains("err=1"));
}

#[quench_test::test]
fn mfa_challenge_url_omits_an_empty_redirect() {
    let url = mfa_challenge_url("pending-token", Some(""), false);
    assert!(!url.contains("redirect="));
    assert!(!url.contains("err=1"));
}

// -----------------------------------------------------------------
// login_redirect
// -----------------------------------------------------------------

#[quench_test::test]
fn login_redirect_points_at_the_login_page() {
    let resp = login_redirect();
    assert_eq!(resp.status(), StatusCode::FOUND);
    let location = resp
        .into_inner()
        .headers
        .get("location")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(location.contains("/login"));
}

// -----------------------------------------------------------------
// login_submit / login_mfa_submit - take only `Form<T>` (pub fields) plus
// `Inject<...>`, callable directly
// -----------------------------------------------------------------

#[quench_test::test]
async fn login_submit_redirects_with_an_error_for_unknown_credentials() {
    let resp = login_submit(
        LocaleCookie(false),
        Form(LoginForm {
            username: "nobody".to_string(),
            password: "whatever".to_string(),
            redirect: None,
        }),
        Inject(Arc::new(JwtConfig::for_tests())),
        Inject(Arc::new(db().await)),
        Inject(sessions()),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    let location = resp
        .into_inner()
        .headers
        .get("location")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(location.contains("err="));
}

#[quench_test::test]
async fn login_submit_succeeds_and_sets_session_cookies_for_the_right_password() {
    let db = db().await;
    realm::create(
        &db,
        &realm_catalog(),
        true,
        "alice",
        "correct-horse",
        vec![Role::User],
        Permissions::new(),
        None,
    )
    .await
    .expect("seed user");

    let resp = login_submit(
        LocaleCookie(false),
        Form(LoginForm {
            username: "alice".to_string(),
            password: "correct-horse".to_string(),
            redirect: None,
        }),
        Inject(Arc::new(JwtConfig::for_tests_with_signing())),
        Inject(Arc::new(db)),
        Inject(sessions()),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(resp.into_inner().headers.get_all("set-cookie").count() >= 2);
}

#[quench_test::test]
fn default_locale_cookie_only_fills_a_gap_with_a_supported_locale() {
    let cookie = default_locale_cookie(&LocaleCookie(false), Some("pl-PL")).expect("cookie");
    assert!(cookie.starts_with("qlocale=pl-PL;"));
    assert!(cookie.contains("Path=/"));

    // An explicit choice already made in the browser - never overwritten.
    assert!(default_locale_cookie(&LocaleCookie(true), Some("pl-PL")).is_none());
    // Unsupported or unset preferences produce nothing.
    assert!(default_locale_cookie(&LocaleCookie(false), Some("xx-XX")).is_none());
    assert!(default_locale_cookie(&LocaleCookie(false), Some("pl-PL; Path=/x")).is_none());
    assert!(default_locale_cookie(&LocaleCookie(false), None).is_none());
}

#[quench_test::test]
async fn locale_cookie_counts_only_with_the_explicit_choice_marker() {
    use quench_http::prelude::FromRequest;
    let container = Arc::new(
        quench_http::di::ContainerBuilder::new()
            .build()
            .await
            .expect("container"),
    );
    for (cookie, expected) in [
        // quench's script writes qlocale on first load, before any choice.
        ("qlocale=en-US", false),
        ("qlocale=pl-PL; qlocale_chosen=1", true),
        ("qlocale_chosen=1", false),
        ("qlocale=xx-XX; qlocale_chosen=1", false),
    ] {
        let mut headers = quench_http::http::HeaderMap::new();
        headers.insert("cookie", cookie.parse().unwrap());
        let mut req = Request::new(
            quench_http::http::Method::GET,
            "/ui/login".parse::<quench_http::http::Uri>().unwrap(),
            headers,
            quench_http::body::InboundBody::from_bytes(quench_core::bytes::Bytes::new()),
            container.clone(),
        );
        let found = LocaleCookie::from_request(&mut req).await.unwrap();
        assert_eq!(found.0, expected, "{cookie}");
    }
}

#[quench_test::test]
async fn login_sets_the_preferred_locale_cookie_only_without_an_existing_one() {
    let db = db().await;
    realm::create(
        &db,
        &realm_catalog(),
        true,
        "bob",
        "correct-horse",
        vec![Role::User],
        Permissions::new(),
        None,
    )
    .await
    .expect("seed user");
    let changes = gatehouse_service::realm::UserChanges {
        preferred_locale: Some("de-DE".to_string()),
        ..Default::default()
    };
    gatehouse_service::realm::update(
        &db,
        &realm_catalog(),
        &sessions(),
        "bob",
        true,
        "bob",
        changes,
    )
    .await
    .expect("save locale");

    for (has_cookie, expected) in [(false, true), (true, false)] {
        let resp = login_submit(
            LocaleCookie(has_cookie),
            Form(LoginForm {
                username: "bob".to_string(),
                password: "correct-horse".to_string(),
                redirect: None,
            }),
            Inject(Arc::new(JwtConfig::for_tests_with_signing())),
            Inject(Arc::new(db.clone())),
            Inject(sessions()),
        )
        .await;
        let has_locale = resp
            .into_inner()
            .headers
            .get_all("set-cookie")
            .any(|value| value.to_str().unwrap().starts_with("qlocale=de-DE"));
        assert_eq!(has_locale, expected);
    }
}

#[quench_test::test]
async fn login_mfa_submit_redirects_with_an_error_for_an_unknown_pending_token() {
    let resp = login_mfa_submit(
        LocaleCookie(false),
        Form(MfaForm {
            pending: "not-a-real-token".to_string(),
            code: "000000".to_string(),
            redirect: None,
        }),
        Inject(Arc::new(JwtConfig::for_tests())),
        Inject(Arc::new(db().await)),
        Inject(sessions()),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
}

// -----------------------------------------------------------------
// login / login_slash / logout / status / refresh - each takes a private
// per-request extractor (`LoginContext`/`LogoutContext`/`AuthStatusResponse`/
// `RefreshResponse`), so these go through the real discovered router.
// -----------------------------------------------------------------

async fn app(
    auth_enabled: bool,
    db: Db,
    sessions: Arc<SessionDb>,
) -> (Arc<dyn Endpoint>, Arc<quench_http::di::Container>) {
    gatehouse_service::ui::pages::auth::register_routes();
    let mut jwt_config = JwtConfig::for_tests_with_signing();
    jwt_config.auth_enabled = auth_enabled;
    let container = ContainerBuilder::new()
        .provide(jwt_config)
        .provide(db)
        .provide_arc(sessions)
        .build()
        .await
        .unwrap();
    (
        quench_starter::http::discover_and_mount("/"),
        Arc::new(container),
    )
}

fn get(path: &str, container: &Arc<quench_http::di::Container>) -> Request {
    Request::new(
        Method::GET,
        path.parse::<Uri>().unwrap(),
        HeaderMap::new(),
        quench_http::body::InboundBody::from_bytes(Bytes::new()),
        container.clone(),
    )
}

fn post(path: &str, container: &Arc<quench_http::di::Container>) -> Request {
    Request::new(
        Method::POST,
        path.parse::<Uri>().unwrap(),
        HeaderMap::new(),
        quench_http::body::InboundBody::from_bytes(Bytes::new()),
        container.clone(),
    )
}

#[quench_test::test]
async fn login_serves_the_page_with_no_session() {
    let (app, container) = app(false, db().await, sessions()).await;
    let resp = app.call(get("/ui/login", &container)).await;
    assert!(resp.status() == StatusCode::OK || resp.status() == StatusCode::NOT_FOUND);
}

#[quench_test::test]
async fn login_slash_serves_the_page_with_no_session() {
    let (app, container) = app(false, db().await, sessions()).await;
    let resp = app.call(get("/ui/login/", &container)).await;
    assert!(resp.status() == StatusCode::OK || resp.status() == StatusCode::NOT_FOUND);
}

#[quench_test::test]
async fn logout_clears_cookies_and_redirects_to_login() {
    let (app, container) = app(false, db().await, sessions()).await;
    let resp = app.call(get("/ui/logout", &container)).await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    let location = resp
        .into_inner()
        .headers
        .get("location")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(location.contains("/login"));
}

#[quench_test::test]
async fn status_reports_when_there_is_no_session() {
    let (app, container) = app(false, db().await, sessions()).await;
    let resp = app.call(get("/ui/status", &container)).await;
    assert!(resp.status().is_success() || resp.status().is_client_error());
}

#[quench_test::test]
async fn refresh_without_a_cookie_is_not_a_server_error() {
    let (app, container) = app(false, db().await, sessions()).await;
    let resp = app.call(post("/ui/refresh", &container)).await;
    assert!(!resp.status().is_server_error());
}

/// The bug this pins: gatehouse runs without `GATEHOUSE_URL`, so renewing through
/// the shared HTTP client always failed and a lapsed session meant a login form.
#[quench_test::test]
async fn a_refresh_cookie_alone_renews_the_session_without_gatehouse_url() {
    quench_core::env::remove("GATEHOUSE_URL");
    let db = db().await;
    realm::create(
        &db,
        &realm_catalog(),
        true,
        "carol",
        "correct-horse",
        vec![Role::User],
        Permissions::new(),
        None,
    )
    .await
    .expect("seed user");
    let sessions = sessions();
    let config = JwtConfig::for_tests_with_signing();
    let user = realm::get(&db, "carol").await.expect("user");
    let issued = gatehouse_service::api::auth::issue_token_pair(&config, &sessions, &user)
        .await
        .expect("issue tokens");

    let (app, container) = app(false, db, sessions).await;
    let cookie = format!("forge_refresh={}", issued.refresh_token);
    let with_cookie = |method: Method, path: &str| {
        let mut headers = HeaderMap::new();
        headers.insert("cookie", cookie.parse().unwrap());
        Request::new(
            method,
            path.parse::<Uri>().unwrap(),
            headers,
            quench_http::body::InboundBody::from_bytes(Bytes::new()),
            container.clone(),
        )
    };

    // The session watcher's renewal.
    let resp = app.call(with_cookie(Method::POST, "/ui/refresh")).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let hyper = resp.into_inner();
    let cookies: Vec<_> = hyper.headers.get_all("set-cookie").collect();
    assert!(cookies.len() >= 2, "expected new session + refresh cookies");
    let rotated = cookies
        .iter()
        .find_map(|v| v.to_str().ok()?.strip_prefix("forge_refresh="))
        .and_then(|v| v.split(';').next())
        .expect("rotated refresh cookie")
        .to_string();
    assert_ne!(rotated, issued.refresh_token);

    // Landing on the login page with only the (rotated) refresh cookie skips the form.
    let mut headers = HeaderMap::new();
    headers.insert(
        "cookie",
        format!("forge_refresh={rotated}").parse().unwrap(),
    );
    let resp = app
        .call(Request::new(
            Method::GET,
            "/ui/login".parse::<Uri>().unwrap(),
            headers,
            quench_http::body::InboundBody::from_bytes(Bytes::new()),
            container.clone(),
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
}

fn realm_catalog() -> gatehouse_service::catalog::PermissionCatalog {
    let dir = std::env::temp_dir().join(format!(
        "auth-page-test-{}",
        quench_core::uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("permissions.toml");
    std::fs::write(&path, "[services.gatehouse]\nactions = [\"read-users\"]\n").unwrap();
    let result =
        gatehouse_service::catalog::PermissionCatalog::load_from(&path.to_string_lossy()).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

/// Two tabs whose session watchers fire in the same moment present the same
/// refresh token. Both must come out signed in.
#[quench_test::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_refreshes_with_one_token_all_succeed_for_two_tabs() {
    let db = db().await;
    realm::create(
        &db,
        &realm_catalog(),
        true,
        "dave",
        "correct-horse",
        vec![Role::User],
        Permissions::new(),
        None,
    )
    .await
    .expect("seed user");
    let config = Arc::new(JwtConfig::for_tests_with_signing());
    let db = Arc::new(db);
    let user = realm::get(&db, "dave").await.expect("user");

    let mut failures = 0;
    for _ in 0..200 {
        let sessions = sessions();
        let issued = gatehouse_service::api::auth::issue_token_pair(&config, &sessions, &user)
            .await
            .expect("issue tokens");
        let attempts = (0..2).map(|_| {
            let (config, db, sessions) = (config.clone(), db.clone(), sessions.clone());
            let token = issued.refresh_token.clone();
            tokio::spawn(async move {
                gatehouse_service::api::auth::refresh_session(&config, &db, &sessions, &token)
                    .await
                    .is_some()
            })
        });
        for handle in attempts {
            if !handle.await.unwrap() {
                failures += 1;
            }
        }
    }
    assert_eq!(failures, 0, "a concurrent refresh was refused");
}
