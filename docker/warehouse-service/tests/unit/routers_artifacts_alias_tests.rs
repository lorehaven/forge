//! The deprecated `/api/v1/apk` routes: every call is marked with `Deprecation`/`Link` (and
//! `Sunset` once a date is scheduled), and `FEATURE_APK_ALIAS_ENABLED=false` answers `410 Gone`.

use crate::support;
use quench_core::bytes::Bytes;
use quench_http::di::ContainerBuilder;
use quench_http::endpoint::Endpoint;
use quench_http::http::{HeaderMap, Method, StatusCode, Uri};
use quench_http::prelude::wrap;
use quench_http::request::Request;
use quench_http::response::Response;
use std::sync::Arc;
use warehouse_service::routers::artifacts::alias::{Deprecation, alias_enabled, alias_sunset};

struct Ok200;

#[quench_core::async_trait]
impl Endpoint for Ok200 {
    async fn call(&self, _req: Request) -> Response {
        Response::new(StatusCode::OK)
    }
}

/// Holds the env lock and puts the alias flags back to their defaults on drop.
struct AliasEnv {
    _guard: std::sync::MutexGuard<'static, ()>,
}

impl AliasEnv {
    fn new() -> Self {
        let guard = support::storage_env_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        Self::reset();
        Self { _guard: guard }
    }

    fn reset() {
        quench_core::env::remove("FEATURE_APK_ALIAS_ENABLED");
        quench_core::env::remove("APK_ALIAS_SUNSET");
    }
}

impl Drop for AliasEnv {
    fn drop(&mut self) {
        Self::reset();
    }
}

async fn call(path: &str) -> quench_http::response::Response {
    let app = wrap(
        Arc::new(Ok200) as Arc<dyn Endpoint>,
        Deprecation::new("/warehouse/api/v1/artifacts"),
    );
    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());
    let mut headers = HeaderMap::new();
    headers.insert("user-agent", "pedlar/1.0".parse().unwrap());
    app.call(Request::new(
        Method::GET,
        path.parse::<Uri>().unwrap(),
        headers,
        quench_http::body::InboundBody::from_bytes(Bytes::new()),
        container,
    ))
    .await
}

#[test]
fn the_alias_is_on_by_default_and_has_no_sunset() {
    let _env = AliasEnv::new();
    assert!(alias_enabled());
    assert_eq!(alias_sunset(), None);
}

#[test]
fn a_blank_sunset_counts_as_unscheduled() {
    let _env = AliasEnv::new();
    quench_core::env::set("APK_ALIAS_SUNSET", "   ");
    assert_eq!(alias_sunset(), None);
}

#[tokio::test]
async fn a_call_still_succeeds_but_is_marked_deprecated() {
    let _env = AliasEnv::new();
    let resp = call("/warehouse/api/v1/apk").await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.headers().get("deprecation").unwrap(), "true");
    assert_eq!(
        resp.headers().get("link").unwrap(),
        "</warehouse/api/v1/artifacts>; rel=\"successor-version\""
    );
    assert!(resp.headers().get("sunset").is_none());
}

#[tokio::test]
async fn a_scheduled_removal_date_is_advertised_as_sunset() {
    let _env = AliasEnv::new();
    quench_core::env::set("APK_ALIAS_SUNSET", "Thu, 31 Dec 2026 00:00:00 GMT");
    let resp = call("/warehouse/api/v1/apk/com.example.app").await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers().get("sunset").unwrap(),
        "Thu, 31 Dec 2026 00:00:00 GMT"
    );
}

#[tokio::test]
async fn switching_the_alias_off_answers_gone_and_never_reaches_the_handler() {
    let _env = AliasEnv::new();
    quench_core::env::set("FEATURE_APK_ALIAS_ENABLED", "false");
    let resp = call("/warehouse/api/v1/apk").await;
    assert_eq!(resp.status(), StatusCode::GONE);
    assert!(resp.headers().get("deprecation").is_none());
}
