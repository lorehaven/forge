//! `stop_instance` - the DELETE handler, its permission check and its
//! not-found/error mapping.

use crate::env_support::env_lock;
use actix_web::http::StatusCode;
use actix_web::web::Data;
use actix_web::{App, test};
use async_trait::async_trait;
use quench_auth::prelude::JwtConfig;
use std::sync::Arc;
use switchboard_service::routers::vllm::engine::VllmEngine;
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

fn engine_data(stop_result: Result<(), String>) -> Data<Arc<dyn VllmEngine>> {
    let engine: Arc<dyn VllmEngine> = Arc::new(StubEngine { stop_result });
    Data::new(engine)
}

#[actix_web::test]
async fn stop_instance_is_forbidden_without_the_stop_permission() {
    let _guard = env_lock().lock().await;
    unsafe { std::env::set_var("SERVICE_AUTH_ENABLED", "true") };

    let app = test::init_service(
        App::new()
            .app_data(Data::new(JwtConfig::for_tests()))
            .app_data(engine_data(Ok(())))
            .service(switchboard_service::routers::vllm::stop::stop_instance),
    )
    .await;

    let req = test::TestRequest::delete()
        .uri("/instances/abc")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    unsafe { std::env::remove_var("SERVICE_AUTH_ENABLED") };
}

#[actix_web::test]
async fn stop_instance_succeeds_and_returns_the_confirm_stop_markup() {
    let _guard = env_lock().lock().await;
    unsafe { std::env::set_var("SERVICE_AUTH_ENABLED", "false") };

    let app = test::init_service(
        App::new()
            .app_data(Data::new(JwtConfig::for_tests()))
            .app_data(engine_data(Ok(())))
            .service(switchboard_service::routers::vllm::stop::stop_instance),
    )
    .await;

    let req = test::TestRequest::delete()
        .uri("/instances/abc")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body = test::read_body(resp).await;
    let html = String::from_utf8(body.to_vec()).expect("utf8");
    assert!(html.contains("confirm-stop-instance-modal"));

    unsafe { std::env::remove_var("SERVICE_AUTH_ENABLED") };
}

#[actix_web::test]
async fn stop_instance_maps_a_not_found_error_to_404() {
    let _guard = env_lock().lock().await;
    unsafe { std::env::set_var("SERVICE_AUTH_ENABLED", "false") };

    let app = test::init_service(
        App::new()
            .app_data(Data::new(JwtConfig::for_tests()))
            .app_data(engine_data(Err("instance not found".to_string())))
            .service(switchboard_service::routers::vllm::stop::stop_instance),
    )
    .await;

    let req = test::TestRequest::delete()
        .uri("/instances/missing")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    unsafe { std::env::remove_var("SERVICE_AUTH_ENABLED") };
}

#[actix_web::test]
async fn stop_instance_maps_any_other_error_to_500() {
    let _guard = env_lock().lock().await;
    unsafe { std::env::set_var("SERVICE_AUTH_ENABLED", "false") };

    let app = test::init_service(
        App::new()
            .app_data(Data::new(JwtConfig::for_tests()))
            .app_data(engine_data(Err("process would not die".to_string())))
            .service(switchboard_service::routers::vllm::stop::stop_instance),
    )
    .await;

    let req = test::TestRequest::delete()
        .uri("/instances/stuck")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);

    unsafe { std::env::remove_var("SERVICE_AUTH_ENABLED") };
}
