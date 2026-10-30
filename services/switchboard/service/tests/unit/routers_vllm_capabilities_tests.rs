//! `GET /api/v1/vllm/capabilities`: what the client reads once to decide what to show.

use crate::env_support::env_lock;
use quench_auth::domain::jwt::JwtConfig;
use quench_http::prelude::Inject;
use std::sync::Arc;
use switchboard_service::routers::models::mod_impl::OptionalClaims;
use switchboard_service::routers::vllm::capabilities::{get_capabilities, get_vllm_namespace};

fn config(auth_enabled: bool) -> Inject<JwtConfig> {
    let mut config = JwtConfig::for_tests();
    config.auth_enabled = auth_enabled;
    Inject(Arc::new(config))
}

#[quench_test::test]
async fn with_auth_off_everything_is_allowed() {
    let _guard = env_lock().lock().await;
    quench_core::env::remove("VLLM_MANAGEMENT_MODE");
    let caps = get_capabilities(OptionalClaims(None), config(false))
        .await
        .0;
    assert!(caps.can_launch && caps.can_stop && caps.can_delete_model);
    assert_eq!(caps.namespace, "native");
}

#[quench_test::test]
async fn with_auth_on_a_visitor_without_claims_may_do_nothing() {
    let _guard = env_lock().lock().await;
    let caps = get_capabilities(OptionalClaims(None), config(true)).await.0;
    assert!(!caps.can_launch && !caps.can_stop && !caps.can_delete_model);
}

#[quench_test::test]
async fn the_namespace_follows_the_engine() {
    let _guard = env_lock().lock().await;
    quench_core::env::set("VLLM_MANAGEMENT_MODE", "mock");
    assert_eq!(get_vllm_namespace(), "mock");
    quench_core::env::remove("VLLM_MANAGEMENT_MODE");
    assert_eq!(get_vllm_namespace(), "native");
}

#[quench_test::test]
fn the_answer_has_the_fields_the_client_reads() {
    let caps = switchboard_service::routers::vllm::capabilities::Capabilities {
        models_dashboard: true,
        vllm_management: false,
        can_launch: true,
        can_stop: false,
        can_delete_model: true,
        namespace: "ml".into(),
    };
    let json = quench_serde::json::to_string(&caps);
    for key in [
        "models_dashboard",
        "vllm_management",
        "can_launch",
        "can_stop",
        "can_delete_model",
        "namespace",
    ] {
        assert!(json.contains(key), "{json}");
    }
}
