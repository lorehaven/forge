use welder::backend::Backend;
use welder::backend::switchboard::SwitchboardBackend;

#[test]
fn initialize_is_always_ok() {
    let backend = SwitchboardBackend::new("http://127.0.0.1:1".to_string());
    assert!(backend.initialize().is_ok());
}

#[test]
fn is_running_is_true_for_a_reachable_host() {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let backend = SwitchboardBackend::new(format!("http://127.0.0.1:{port}"));
    assert!(backend.is_running());
}

#[test]
fn is_running_is_false_for_an_unreachable_host() {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind");
    let port = listener.local_addr().expect("addr").port();
    drop(listener);
    let backend = SwitchboardBackend::new(format!("http://127.0.0.1:{port}"));
    assert!(!backend.is_running());
}

#[test]
fn is_running_is_false_for_an_unparseable_url() {
    let backend = SwitchboardBackend::new("not a url".to_string());
    assert!(!backend.is_running());
}

#[test]
fn is_running_is_false_when_the_url_has_no_host() {
    let backend = SwitchboardBackend::new("file:///etc/passwd".to_string());
    assert!(!backend.is_running());
}

#[test]
fn initialized_prints_the_banner_without_panicking() {
    let backend = SwitchboardBackend::new("http://127.0.0.1:1".to_string());
    backend.initialized();
}
