use welder::backend::Backend;
use welder::backend::ollama::OllamaBackend;

#[test]
fn is_running_is_true_for_a_reachable_address() {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind");
    let addr = listener.local_addr().expect("addr");
    let backend = OllamaBackend::new(addr.to_string());
    assert!(backend.is_running());
}

#[test]
fn is_running_is_false_for_an_unreachable_address() {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind");
    let addr = listener.local_addr().expect("addr");
    drop(listener);
    let backend = OllamaBackend::new(addr.to_string());
    assert!(!backend.is_running());
}

#[test]
fn initialize_is_a_no_op_when_already_running() {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind");
    let addr = listener.local_addr().expect("addr");
    let backend = OllamaBackend::new(addr.to_string());
    assert!(backend.initialize().is_ok());
}

/// This sandbox has no `ollama` binary on `PATH`, so when nothing is already
/// listening, `require_binary` is the deterministic failure mode.
#[test]
fn initialize_errors_without_the_ollama_binary_when_nothing_is_listening() {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind");
    let addr = listener.local_addr().expect("addr");
    drop(listener);
    let backend = OllamaBackend::new(addr.to_string());
    assert!(backend.initialize().is_err());
}

#[test]
fn initialized_prints_the_banner_without_panicking() {
    let backend = OllamaBackend::new("127.0.0.1:1".to_string());
    backend.initialized();
}
