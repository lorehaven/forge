//! `wrap_auth` on the crates registry and `/admin`: anonymous callers are refused, a read-only
//! identity may read crates but never write or run GC, `warehouse:write` may do both.

use quench_auth::domain::jwt::JwtConfig;
use quench_core::bytes::Bytes;
use quench_http::body::InboundBody;
use quench_http::di::{Container, ContainerBuilder};
use quench_http::endpoint::Endpoint;
use quench_http::http::{HeaderMap, Method, StatusCode, Uri};
use quench_http::request::Request;
use quench_http::response::Response;
use std::sync::Arc;
use warehouse_service::routers::{admin, crates};

fn config() -> JwtConfig {
    let mut config = JwtConfig::for_tests_with_signing();
    config.service_name = "warehouse".to_string();
    config.audiences = vec!["warehouse".to_string()];
    config.auth_enabled = true;
    config
}

struct Ok200;

#[quench_core::async_trait]
impl Endpoint for Ok200 {
    async fn call(&self, _req: Request) -> Response {
        Response::new(StatusCode::OK)
    }
}

fn crates_app(config: JwtConfig, base_path: &str) -> Arc<dyn Endpoint> {
    crates::wrap_auth(Arc::new(Ok200), config, base_path, false)
}

fn crates_app_authed_reads(config: JwtConfig) -> Arc<dyn Endpoint> {
    crates::wrap_auth(Arc::new(Ok200), config, "", true)
}

fn admin_app(config: JwtConfig, base_path: &str) -> Arc<dyn Endpoint> {
    admin::wrap_auth(Arc::new(Ok200), config, base_path)
}

async fn container() -> Arc<Container> {
    Arc::new(ContainerBuilder::new().build().await.unwrap())
}

fn request(method: Method, path: &str, container: &Arc<Container>, token: Option<&str>) -> Request {
    let mut headers = HeaderMap::new();
    if let Some(token) = token {
        headers.insert("authorization", format!("Bearer {token}").parse().unwrap());
    }
    Request::new(
        method,
        path.parse::<Uri>().unwrap(),
        headers,
        InboundBody::from_bytes(Bytes::new()),
        container.clone(),
    )
}

async fn token(config: &JwtConfig, role: &str) -> String {
    config
        .issue_access_token("someone".to_string(), role.to_string(), None)
        .await
        .unwrap()
}

#[tokio::test]
async fn anonymous_crates_writes_are_unauthorized() {
    let app = crates_app(config(), "");
    let c = container().await;

    for (method, path) in [
        (Method::PUT, "/api/v1/crates/new"),
        (Method::DELETE, "/api/v1/crates/foo/1.0.0/yank"),
        (Method::PUT, "/api/v1/crates/foo/owners"),
    ] {
        let resp = app.call(request(method.clone(), path, &c, None)).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED, "{method} {path}");
    }
}

#[tokio::test]
async fn anonymous_crates_reads_stay_open_by_default() {
    let app = crates_app(config(), "");
    let c = container().await;

    for path in [
        "/index/config.json",
        "/index/fo/o-/foo-bar",
        "/api/v1/crates?q=foo",
        "/api/v1/crates/foo/1.0.0/download",
        "/api/v1/crates/foo/owners",
    ] {
        let resp = app.call(request(Method::GET, path, &c, None)).await;
        assert_eq!(resp.status(), StatusCode::OK, "{path}");
    }
}

#[tokio::test]
async fn the_sparse_index_cannot_be_written_anonymously() {
    let app = crates_app(config(), "");
    let c = container().await;
    let resp = app
        .call(request(Method::PUT, "/index/config.json", &c, None))
        .await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn opting_in_makes_index_and_download_need_an_identity() {
    let cfg = config();
    let t = token(&cfg, "warehouse:read").await;
    let app = crates_app_authed_reads(cfg);
    let c = container().await;

    for path in [
        "/index/config.json",
        "/index/fo/o-/foo-bar",
        "/api/v1/crates/foo/1.0.0/download",
    ] {
        let anon = app.call(request(Method::GET, path, &c, None)).await;
        assert_eq!(anon.status(), StatusCode::UNAUTHORIZED, "{path}");
        let authed = app.call(request(Method::GET, path, &c, Some(&t))).await;
        assert_eq!(authed.status(), StatusCode::OK, "{path}");
    }
}

#[tokio::test]
async fn a_garbage_token_is_unauthorized() {
    let app = crates_app(config(), "");
    let c = container().await;
    let resp = app
        .call(request(Method::PUT, "/api/v1/crates/new", &c, Some("nope")))
        .await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn read_grant_cannot_publish_yank_or_edit_owners() {
    let cfg = config();
    let t = token(&cfg, "warehouse:read").await;
    let app = crates_app(cfg, "");
    let c = container().await;

    for (method, path) in [
        (Method::PUT, "/api/v1/crates/new"),
        (Method::DELETE, "/api/v1/crates/foo/1.0.0/yank"),
        (Method::PUT, "/api/v1/crates/foo/1.0.0/unyank"),
        (Method::PUT, "/api/v1/crates/foo/owners"),
        (Method::DELETE, "/api/v1/crates/foo/owners"),
    ] {
        let resp = app.call(request(method.clone(), path, &c, Some(&t))).await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN, "{method} {path}");
    }
}

#[tokio::test]
async fn write_grant_may_publish_and_yank() {
    let cfg = config();
    let t = token(&cfg, "warehouse:write").await;
    let app = crates_app(cfg, "");
    let c = container().await;

    for (method, path) in [
        (Method::PUT, "/api/v1/crates/new"),
        (Method::DELETE, "/api/v1/crates/foo/1.0.0/yank"),
    ] {
        let resp = app.call(request(method.clone(), path, &c, Some(&t))).await;
        assert_eq!(resp.status(), StatusCode::OK, "{method} {path}");
    }
}

#[tokio::test]
async fn crates_auth_follows_the_base_path_and_leaves_other_paths_alone() {
    let app = crates_app(config(), "/warehouse");
    let c = container().await;

    let unprefixed = app
        .call(request(Method::PUT, "/api/v1/crates/new", &c, None))
        .await;
    assert_eq!(unprefixed.status(), StatusCode::OK);

    let guarded = app
        .call(request(
            Method::PUT,
            "/warehouse/api/v1/crates/new",
            &c,
            None,
        ))
        .await;
    assert_eq!(guarded.status(), StatusCode::UNAUTHORIZED);

    let elsewhere = app
        .call(request(Method::GET, "/warehouse/ui/home", &c, None))
        .await;
    assert_eq!(elsewhere.status(), StatusCode::OK);
}

#[tokio::test]
async fn disabled_auth_bypasses_both_wrappers() {
    let mut cfg = config();
    cfg.auth_enabled = false;
    let c = container().await;

    let resp = crates_app(cfg.clone(), "")
        .call(request(Method::PUT, "/api/v1/crates/new", &c, None))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);

    let resp = admin_app(cfg, "")
        .call(request(Method::POST, "/admin/crates/gc", &c, None))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn anonymous_admin_gc_is_unauthorized() {
    let app = admin_app(config(), "");
    let c = container().await;

    for path in ["/admin/crates/gc", "/admin/docker/gc"] {
        let resp = app.call(request(Method::POST, path, &c, None)).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED, "{path}");
    }
}

#[tokio::test]
async fn read_grant_cannot_run_admin_gc() {
    let cfg = config();
    let t = token(&cfg, "warehouse:read").await;
    let app = admin_app(cfg, "");
    let c = container().await;

    for path in ["/admin/crates/gc", "/admin/docker/gc"] {
        let resp = app.call(request(Method::POST, path, &c, Some(&t))).await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN, "{path}");
    }
}

#[tokio::test]
async fn write_grant_and_wildcard_role_can_run_admin_gc() {
    let cfg = config();
    let write = token(&cfg, "warehouse:write").await;
    let admin_token = token(&cfg, "admin").await;
    let app = admin_app(cfg, "");
    let c = container().await;

    for t in [&write, &admin_token] {
        let resp = app
            .call(request(Method::POST, "/admin/crates/gc", &c, Some(t)))
            .await;
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
