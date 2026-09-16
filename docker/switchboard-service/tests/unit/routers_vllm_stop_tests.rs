//! `stop_instance` - the DELETE handler, its permission check and its
//! not-found/error mapping.

use async_trait::async_trait;
use http::StatusCode;
use http_body_util::BodyExt;
use quench_auth::domain::jwt::JwtConfig;
use quench_http::prelude::{Inject, Path};
use std::sync::Arc;
use switchboard_service::routers::models::mod_impl::OptionalClaims;
use switchboard_service::routers::vllm::engine::VllmEngine;
use switchboard_service::routers::vllm::stop::stop_instance;
use switchboard_service::routers::vllm::types::{LaunchRequest, VllmInstance};

struct StubEngine {
    stop_result: Result<(), String>,
}

#[async_trait]
impl VllmEngine for StubEngine {
    async fn list_instances(&self) -> Result<Vec<VllmInstance>, String> {
        unimplemented!()
    }
    async fn launch_instance(&self, _req: LaunchRequest) -> Result<VllmInstance, String> {
        unimplemented!()
    }
    async fn stop_instance(&self, _id: String) -> Result<(), String> {
        self.stop_result.clone()
    }
}

fn engine(stop_result: Result<(), String>) -> Inject<Arc<dyn VllmEngine>> {
    let engine: Arc<dyn VllmEngine> = Arc::new(StubEngine { stop_result });
    Inject(Arc::new(engine))
}

async fn body_text(resp: quench_http::response::Response) -> String {
    let collected = resp
        .into_hyper()
        .into_body()
        .collect()
        .await
        .expect("body collects");
    String::from_utf8(collected.to_bytes().to_vec()).expect("utf8")
}

#[tokio::test]
async fn stop_instance_is_forbidden_without_the_stop_permission() {
    let mut config = JwtConfig::for_tests();
    config.auth_enabled = true;

    let resp = stop_instance(
        OptionalClaims(None),
        Inject(Arc::new(config)),
        Path("abc".to_string()),
        engine(Ok(())),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn stop_instance_succeeds_and_returns_the_confirm_stop_markup() {
    let mut config = JwtConfig::for_tests();
    config.auth_enabled = false;

    let resp = stop_instance(
        OptionalClaims(None),
        Inject(Arc::new(config)),
        Path("abc".to_string()),
        engine(Ok(())),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);

    let html = body_text(resp).await;
    assert!(html.contains("confirm-stop-instance-modal"));
}

#[tokio::test]
async fn stop_instance_maps_a_not_found_error_to_404() {
    let mut config = JwtConfig::for_tests();
    config.auth_enabled = false;

    let resp = stop_instance(
        OptionalClaims(None),
        Inject(Arc::new(config)),
        Path("missing".to_string()),
        engine(Err("instance not found".to_string())),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn stop_instance_maps_any_other_error_to_500() {
    let mut config = JwtConfig::for_tests();
    config.auth_enabled = false;

    let resp = stop_instance(
        OptionalClaims(None),
        Inject(Arc::new(config)),
        Path("stuck".to_string()),
        engine(Err("process would not die".to_string())),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
}
