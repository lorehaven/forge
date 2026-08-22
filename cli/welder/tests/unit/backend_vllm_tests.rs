use welder::backend::Backend;
use welder::backend::vllm::VllmBackend;

#[test]
fn vllm_backend_initialize_and_is_running_are_always_ok() {
    let backend = VllmBackend::new();
    assert!(backend.initialize().is_ok());
    assert!(backend.is_running());
    backend.initialized();
}

#[test]
fn vllm_backend_default_matches_new() {
    let backend = VllmBackend;
    assert!(backend.is_running());
}
