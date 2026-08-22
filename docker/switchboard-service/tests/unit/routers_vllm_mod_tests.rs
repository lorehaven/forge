//! `init_engine`'s mode dispatch in `routers/vllm/mod.rs`. There's no way to
//! downcast `Arc<dyn VllmEngine>` back to a concrete type, so each case is
//! only checked by calling through to a real implementation without erroring.
//! `MockVllmEngine`'s store is a process-global static also touched by
//! `routers_vllm_mock_tests`, which may be running concurrently in this same
//! test binary, so the mock case here can't assert on the instance list's
//! contents. The `Kubernetes` arm isn't exercised here: reaching it requires
//! a rustls `CryptoProvider` installed process-wide (`kube::Client::try_default`
//! needs one), which nothing in this crate's test binary installs.

use crate::env_support::env_lock;
use switchboard_service::routers::vllm::init_engine;

#[tokio::test]
async fn init_engine_defaults_to_native_with_no_mode_set() {
    let _guard = env_lock().lock().await;
    envmnt::remove("VLLM_MANAGEMENT_MODE");

    // Native's `list_instances` shells out to find real vLLM processes; it
    // must not panic even when none exist.
    let engine = init_engine().await;
    let _ = engine.list_instances().await;
}

#[tokio::test]
async fn init_engine_selects_mock_mode() {
    let _guard = env_lock().lock().await;
    envmnt::set("VLLM_MANAGEMENT_MODE", "mock");

    let engine = init_engine().await;
    let _ = engine.list_instances().await.expect("mock never errors");

    envmnt::remove("VLLM_MANAGEMENT_MODE");
}
