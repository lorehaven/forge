//! Pure/file-I/O helpers in `routers/vllm/native.rs`. `NativeVllmEngine`
//! itself spawns and manages real OS processes, so it's intentionally not
//! exercised here - only the logic around it that doesn't require an actual
//! running vLLM process.

use crate::env_support::env_lock;
use switchboard_service::routers::vllm::native::{
    create_launch_log_path, create_pid_log_path, extract_arg, instance_key, log_indicates_started,
    read_log_tail,
};

fn parts(args: &[&str]) -> Vec<String> {
    args.iter().map(|s| s.to_string()).collect()
}

#[test]
fn extract_arg_finds_the_value_following_a_flag() {
    let args = parts(&["--dtype", "float16", "--port", "8000"]);
    assert_eq!(extract_arg(&args, "--dtype"), Some("float16".to_string()));
    assert_eq!(extract_arg(&args, "--port"), Some("8000".to_string()));
}

#[test]
fn extract_arg_is_none_for_an_absent_or_trailing_flag() {
    let args = parts(&["--dtype", "float16"]);
    assert_eq!(extract_arg(&args, "--missing"), None);
    assert_eq!(extract_arg(&parts(&["--dtype"]), "--dtype"), None);
}

#[test]
fn instance_key_replaces_slashes_and_appends_the_port() {
    assert_eq!(instance_key("meta/llama-3", 8000), "meta--llama-3-8000");
    assert_eq!(instance_key("no-slash", 1234), "no-slash-1234");
}

#[test]
fn create_pid_log_path_replaces_the_log_suffix_with_a_pid_suffix() {
    assert_eq!(
        create_pid_log_path("/var/log/vllm/run.log", 42),
        "/var/log/vllm/run-pid-42.log"
    );
}

#[test]
fn create_pid_log_path_appends_when_there_is_no_log_suffix_to_replace() {
    assert_eq!(
        create_pid_log_path("/var/log/vllm/run", 42),
        "/var/log/vllm/run-pid-42"
    );
}

#[tokio::test]
async fn create_launch_log_path_uses_vllm_log_dir_and_embeds_model_and_port() {
    let _guard = env_lock().lock().await;
    let dir = tempfile::tempdir().expect("tempdir");
    unsafe { std::env::set_var("VLLM_LOG_DIR", dir.path().to_str().unwrap()) };

    let path = create_launch_log_path("meta/llama-3-8b", 8000);

    assert!(path.starts_with(dir.path().to_str().unwrap()));
    assert!(path.contains("meta__llama-3-8b"));
    assert!(path.contains("-8000.log"));
    assert!(dir.path().exists()); // create_dir_all ran

    unsafe { std::env::remove_var("VLLM_LOG_DIR") };
}

#[test]
fn log_indicates_started_true_when_the_marker_line_is_present() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("run.log");
    std::fs::write(&path, "INFO: booting\nINFO: Started server process [123]\n").unwrap();

    assert!(log_indicates_started(path.to_str().unwrap(), 123));
    assert!(!log_indicates_started(path.to_str().unwrap(), 999));
}

#[test]
fn log_indicates_started_is_false_for_a_missing_file() {
    assert!(!log_indicates_started("/does/not/exist.log", 1));
}

#[test]
fn read_log_tail_returns_at_most_the_requested_number_of_trailing_lines() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("run.log");
    std::fs::write(&path, "line1\nline2\nline3\nline4\n").unwrap();

    let tail = read_log_tail(path.to_str().unwrap(), 2).unwrap();
    assert_eq!(tail, "line3\nline4");
}

#[test]
fn read_log_tail_returns_the_whole_file_when_it_has_fewer_lines_than_requested() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("run.log");
    std::fs::write(&path, "only-line\n").unwrap();

    let tail = read_log_tail(path.to_str().unwrap(), 10).unwrap();
    assert_eq!(tail, "only-line");
}

#[test]
fn read_log_tail_is_none_for_a_missing_file() {
    assert!(read_log_tail("/does/not/exist.log", 5).is_none());
}
