//! HTTP-layer helpers shared by the router tests: the app router plus a hand-built request, the same
//! `discover_and_mount` convention the other services' tests use.
#![allow(dead_code)]

use bytes::Bytes;
use http::{HeaderMap, Method, Uri};
use http_body_util::BodyExt;
use quench_auth::domain::jwt::JwtConfig;
use quench_db::prelude::Db;
use quench_http::di::{Container, ContainerBuilder};
use quench_http::endpoint::Endpoint;
use quench_http::request::Request;
use std::sync::Arc;

pub async fn app(container: Container) -> (Arc<dyn Endpoint>, Arc<Container>) {
    (
        quench_starter::http::discover_and_mount("/"),
        Arc::new(container),
    )
}

pub fn container_builder() -> ContainerBuilder {
    ContainerBuilder::new()
}

/// A container with just `JwtConfig` and an in-memory `Db` - the minimum most handlers need.
pub async fn basic_container(jwt_config: JwtConfig, db: Db) -> Container {
    ContainerBuilder::new()
        .provide(jwt_config)
        .provide(db)
        .build()
        .await
        .unwrap()
}

pub fn jwt_config(auth_enabled: bool) -> JwtConfig {
    let mut config = JwtConfig::for_tests();
    config.auth_enabled = auth_enabled;
    config
}

pub fn req(method: Method, path: &str, container: &Arc<Container>) -> Request {
    Request::new(
        method,
        path.parse::<Uri>().unwrap(),
        HeaderMap::new(),
        quench_http::body::InboundBody::from_bytes(Bytes::new()),
        container.clone(),
    )
}

pub async fn body_text(resp: quench_http::response::Response) -> String {
    let collected = resp.into_hyper().into_body().collect().await.expect("body");
    String::from_utf8_lossy(&collected.to_bytes()).into_owned()
}

pub async fn json_body(resp: quench_http::response::Response) -> serde_json::Value {
    let collected = resp.into_hyper().into_body().collect().await.expect("body");
    serde_json::from_slice(&collected.to_bytes()).expect("valid json body")
}

pub fn req_json(method: Method, path: &str, container: &Arc<Container>, json: &str) -> Request {
    let mut headers = HeaderMap::new();
    headers.insert("content-type", "application/json".parse().unwrap());
    Request::new(
        method,
        path.parse::<Uri>().unwrap(),
        headers,
        quench_http::body::InboundBody::from_bytes(Bytes::from(json.to_string())),
        container.clone(),
    )
}

pub fn req_form(
    method: Method,
    path: &str,
    container: &Arc<Container>,
    pairs: &[(&str, &str)],
) -> Request {
    let mut headers = HeaderMap::new();
    headers.insert(
        "content-type",
        "application/x-www-form-urlencoded".parse().unwrap(),
    );
    Request::new(
        method,
        path.parse::<Uri>().unwrap(),
        headers,
        quench_http::body::InboundBody::from_bytes(Bytes::from(
            serde_urlencoded::to_string(pairs).unwrap(),
        )),
        container.clone(),
    )
}

/// A whole Gantry over in-memory parts, with handles to the parts a test scripts.
pub struct Rig {
    pub app: Arc<dyn Endpoint>,
    pub container: Arc<Container>,
    pub cluster: Arc<gantry_service::domain::cluster::FakeCluster>,
    pub registry: Arc<gantry_service::domain::registry::MemoryRegistry>,
}

pub async fn rig(auth_enabled: bool) -> Rig {
    use gantry_service::domain::cluster::FakeCluster;
    use gantry_service::domain::executor::{ClusterExecutor, Executor};
    use gantry_service::domain::operation::MemoryStore;
    use gantry_service::domain::registry::MemoryRegistry;
    use gantry_service::domain::service::Gantry;
    use gantry_service::domain::settings::Settings;

    gantry_service::routers::api::register_routes();
    gantry_service::routers::ui::register_routes();
    let settings = Settings::inert();
    let cluster = Arc::new(FakeCluster::new());
    let registry = Arc::new(MemoryRegistry::default());
    let executor: Arc<dyn Executor> = Arc::new(ClusterExecutor {
        cluster: cluster.clone(),
        settings: settings.clone(),
    });
    let gantry = Gantry::new(
        Arc::new(MemoryStore::new()),
        cluster.clone(),
        executor,
        registry.clone(),
        settings,
    );
    let container = ContainerBuilder::new()
        .provide(jwt_config(auth_enabled))
        .provide(Db::InMemory(quench_db::InMemoryDb::new()))
        .provide(gantry)
        .build()
        .await
        .unwrap();
    let (app, container) = app(container).await;
    Rig {
        app,
        container,
        cluster,
        registry,
    }
}
