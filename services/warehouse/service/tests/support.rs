//! Shared locks for tests that redirect this crate's process-global storage
//! env vars at a tempdir. Each covers a distinct set of env vars, so tests
//! touching unrelated vars don't serialize against each other.
#![allow(dead_code)]

use quench_auth::domain::jwt::JwtConfig;
use quench_core::bytes::Bytes;
use quench_db::prelude::Db;
use quench_http::di::ContainerBuilder;
use quench_http::endpoint::Endpoint;
use quench_http::http::{HeaderMap, Method, Uri};
use quench_http::request::Request;
use std::sync::{Arc, Mutex, OnceLock};

// ---------------------------------------------------------------------------
// HTTP-layer helpers, shared by every `routers_*_tests.rs` file - same
// `discover_and_mount` + hand-built `quench_http::request::Request`
// convention used across every already-migrated service this session
// (sage/workbench/gatehouse/switchboard/conveyor).
// ---------------------------------------------------------------------------

/// Builds the app router plus a bare DI container.
///
/// Callers add whatever `.provide(...)`/`.provide_arc(...)` their route needs before `.build()` -
/// use `container_builder()` directly rather than this if you need to.
#[must_use]
pub fn app(
    container: quench_http::di::Container,
) -> (Arc<dyn Endpoint>, Arc<quench_http::di::Container>) {
    (
        quench_starter::http::discover_and_mount("/"),
        Arc::new(container),
    )
}

#[must_use]
pub fn container_builder() -> ContainerBuilder {
    ContainerBuilder::new()
}

/// A container with just `JwtConfig` and an in-memory `Db` - the minimum
/// most handlers need. Provide more on top with `.provide(...)` before
/// `.build()` if a specific handler needs it.
pub async fn basic_container(jwt_config: JwtConfig, db: Db) -> quench_http::di::Container {
    ContainerBuilder::new()
        .provide(jwt_config)
        .provide(db)
        .build()
        .await
        .unwrap()
}

#[must_use]
pub fn req(method: Method, path: &str, container: &Arc<quench_http::di::Container>) -> Request {
    Request::new(
        method,
        path.parse::<Uri>().unwrap(),
        HeaderMap::new(),
        quench_http::body::InboundBody::from_bytes(Bytes::new()),
        container.clone(),
    )
}

#[must_use]
pub fn json_req(
    method: Method,
    path: &str,
    body: &quench_serde::Value,
    container: &Arc<quench_http::di::Container>,
) -> Request {
    let mut headers = HeaderMap::new();
    headers.insert("content-type", "application/json".parse().unwrap());
    Request::new(
        method,
        path.parse::<Uri>().unwrap(),
        headers,
        quench_http::body::InboundBody::from_bytes(Bytes::from(quench_serde::json::to_vec(body))),
        container.clone(),
    )
}

#[must_use]
pub fn form_req(
    method: Method,
    path: &str,
    pairs: &[(&str, &str)],
    container: &Arc<quench_http::di::Container>,
) -> Request {
    let encoded = quench_serde::urlencoded::to_string(pairs).unwrap();
    let mut headers = HeaderMap::new();
    headers.insert(
        "content-type",
        "application/x-www-form-urlencoded".parse().unwrap(),
    );
    Request::new(
        method,
        path.parse::<Uri>().unwrap(),
        headers,
        quench_http::body::InboundBody::from_bytes(Bytes::from(encoded)),
        container.clone(),
    )
}

/// A request carrying a raw byte body and arbitrary headers - for docker
/// registry blob/manifest pushes and anything authenticated by a signature
/// over exact bytes rather than a parsed form/JSON body.
#[must_use]
pub fn raw_req(
    method: Method,
    path: &str,
    headers: &[(&str, &str)],
    body: &[u8],
    container: &Arc<quench_http::di::Container>,
) -> Request {
    let mut header_map = HeaderMap::new();
    for (name, value) in headers {
        header_map.insert(
            quench_http::http::HeaderName::from_bytes(name.as_bytes()).unwrap(),
            value.parse().unwrap(),
        );
    }
    Request::new(
        method,
        path.parse::<Uri>().unwrap(),
        header_map,
        quench_http::body::InboundBody::from_bytes(Bytes::copy_from_slice(body)),
        container.clone(),
    )
}

pub async fn body_text(resp: quench_http::response::Response) -> String {
    let collected = resp
        .into_inner()
        .body
        .collect_limited(usize::MAX)
        .await
        .expect("body");
    String::from_utf8_lossy(&collected).into_owned()
}

pub async fn body_bytes(resp: quench_http::response::Response) -> Bytes {
    resp.into_inner()
        .body
        .collect_limited(usize::MAX)
        .await
        .expect("body")
}

pub async fn json_body(resp: quench_http::response::Response) -> quench_serde::Value {
    let collected = resp
        .into_inner()
        .body
        .collect_limited(usize::MAX)
        .await
        .expect("body");
    quench_serde::json::from_slice(&collected).expect("valid json body")
}

/// Splits a response into its headers and text body, since `Response` only
/// exposes headers via the consuming `into_hyper()`.
pub async fn parts(
    resp: quench_http::response::Response,
) -> (quench_http::http::HeaderMap, String) {
    let parts = resp.into_inner();
    let collected = parts.body.collect_limited(usize::MAX).await.expect("body");
    (
        parts.headers,
        String::from_utf8_lossy(&collected).into_owned(),
    )
}

#[must_use]
pub fn location(headers: &quench_http::http::HeaderMap) -> String {
    headers
        .get("location")
        .expect("a Location header")
        .to_str()
        .unwrap()
        .to_string()
}

/// Guards `CRATES_STORAGE_PATH`/`STORAGE_PATH`/`ARTIFACT_STORAGE_PATH`.
///
/// Each is read fresh on every call by
/// `warehouse_service::routers::crates_storage_root`/`docker_storage_root`/
/// `artifact_storage_root` - two tests setting different values concurrently
/// would otherwise race each other's storage roots out from under them.
pub fn storage_env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// Guards the docker registry token signing secret env var.
pub fn secret_env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// Guards whatever env var controls blob-retrieve redirect behavior.
pub fn redirect_env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// Points `STORAGE_PATH` (the docker registry's storage root) at a fresh tempdir for one test.
///
/// Holds `storage_env_lock`, so concurrent tests doing the same thing don't race each other's roots.
#[derive(Debug)]
pub struct WithDockerStorageRoot {
    _guard: std::sync::MutexGuard<'static, ()>,
    pub dir: quench_core::tmp::TempDir,
}

impl Default for WithDockerStorageRoot {
    fn default() -> Self {
        Self::new()
    }
}

impl WithDockerStorageRoot {
    pub fn new() -> Self {
        let guard = storage_env_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let dir = quench_core::tmp::TempDir::new().expect("tempdir");
        quench_core::env::set("STORAGE_PATH", dir.path().to_str().unwrap());
        Self { _guard: guard, dir }
    }
}

impl Drop for WithDockerStorageRoot {
    fn drop(&mut self) {
        quench_core::env::remove("STORAGE_PATH");
    }
}

/// Points `CRATES_STORAGE_PATH` (the cargo registry's storage root) at a fresh tempdir for one test.
///
/// Holds `storage_env_lock`, so concurrent tests doing the same thing don't race each other's roots.
#[derive(Debug)]
pub struct WithCratesStorageRoot {
    _guard: std::sync::MutexGuard<'static, ()>,
    pub dir: quench_core::tmp::TempDir,
}

impl Default for WithCratesStorageRoot {
    fn default() -> Self {
        Self::new()
    }
}

impl WithCratesStorageRoot {
    pub fn new() -> Self {
        let guard = storage_env_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let dir = quench_core::tmp::TempDir::new().expect("tempdir");
        quench_core::env::set("CRATES_STORAGE_PATH", dir.path().to_str().unwrap());
        Self { _guard: guard, dir }
    }
}

impl Drop for WithCratesStorageRoot {
    fn drop(&mut self) {
        quench_core::env::remove("CRATES_STORAGE_PATH");
    }
}

/// Points `ARTIFACT_STORAGE_PATH` (the artifact store's root) at a fresh tempdir for one test.
///
/// Holds `storage_env_lock`, so concurrent tests doing the same thing don't race each other's roots.
#[derive(Debug)]
pub struct WithArtifactStorageRoot {
    _guard: std::sync::MutexGuard<'static, ()>,
    pub dir: quench_core::tmp::TempDir,
}

impl Default for WithArtifactStorageRoot {
    fn default() -> Self {
        Self::new()
    }
}

impl WithArtifactStorageRoot {
    pub fn new() -> Self {
        let guard = storage_env_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let dir = quench_core::tmp::TempDir::new().expect("tempdir");
        quench_core::env::set("ARTIFACT_STORAGE_PATH", dir.path().to_str().unwrap());
        Self { _guard: guard, dir }
    }
}

impl Drop for WithArtifactStorageRoot {
    fn drop(&mut self) {
        quench_core::env::remove("ARTIFACT_STORAGE_PATH");
    }
}

/// Turns the rivet registry on and points its storage at a fresh tempdir for one test.
///
/// Holds `storage_env_lock`, so concurrent tests don't race each other's roots or the flag.
#[derive(Debug)]
pub struct WithRivets {
    _guard: std::sync::MutexGuard<'static, ()>,
    pub dir: quench_core::tmp::TempDir,
}

impl Default for WithRivets {
    fn default() -> Self {
        Self::new()
    }
}

impl WithRivets {
    pub fn new() -> Self {
        let guard = storage_env_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let dir = quench_core::tmp::TempDir::new().expect("tempdir");
        quench_core::env::set("RIVET_STORAGE_PATH", dir.path().to_str().unwrap());
        quench_core::env::set("FEATURE_RIVETS_ENABLED", "true");
        Self { _guard: guard, dir }
    }

    /// The registry switched off, still holding the lock so no other test turns it on mid-case.
    #[must_use]
    pub fn disabled() -> Self {
        let this = Self::new();
        quench_core::env::remove("FEATURE_RIVETS_ENABLED");
        this
    }
}

impl Drop for WithRivets {
    fn drop(&mut self) {
        quench_core::env::remove("RIVET_STORAGE_PATH");
        quench_core::env::remove("FEATURE_RIVETS_ENABLED");
        quench_core::env::remove("RIVET_MAX_BYTES");
    }
}
