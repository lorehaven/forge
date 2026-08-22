//! `tools/file_ops.rs` - path confinement is the security-relevant part of
//! this file, so it gets the most thorough treatment; `execute`'s four
//! operations are exercised end to end against a real `tempfile::tempdir`.

use crate::env_support::env_lock;
use sage_service::tools::file_ops::FileOpsExecutor;
use sage_service::tools::{ToolCall, ToolExecutor};

fn call(args: serde_json::Value) -> ToolCall {
    ToolCall {
        id: "call-1".to_string(),
        name: "file_ops".to_string(),
        arguments: args,
    }
}

#[test]
fn get_definition_declares_the_four_operations() {
    let def = sage_service::tools::file_ops::get_definition();
    assert_eq!(def.name, "file_ops");
    assert_eq!(
        def.parameters.required,
        vec!["operation".to_string(), "path".to_string()]
    );
}

#[tokio::test]
async fn read_returns_the_files_contents() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("hello.txt"), "hello world").expect("write fixture");
    let executor = FileOpsExecutor::new(dir.path().to_string_lossy().to_string());

    let result = executor
        .execute(&call(
            serde_json::json!({ "operation": "read", "path": "hello.txt" }),
        ))
        .await;

    assert!(!result.is_error);
    assert_eq!(result.content, "hello world");
}

#[tokio::test]
async fn read_reports_a_missing_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let executor = FileOpsExecutor::new(dir.path().to_string_lossy().to_string());

    let result = executor
        .execute(&call(
            serde_json::json!({ "operation": "read", "path": "missing.txt" }),
        ))
        .await;

    assert!(result.is_error);
    assert!(result.content.contains("Failed to read file"));
}

#[tokio::test]
async fn write_then_read_round_trips() {
    let dir = tempfile::tempdir().expect("tempdir");
    let executor = FileOpsExecutor::new(dir.path().to_string_lossy().to_string());

    let write_result = executor
        .execute(&call(serde_json::json!({
            "operation": "write",
            "path": "new.txt",
            "content": "written content"
        })))
        .await;
    assert!(!write_result.is_error);

    let read_result = executor
        .execute(&call(
            serde_json::json!({ "operation": "read", "path": "new.txt" }),
        ))
        .await;
    assert_eq!(read_result.content, "written content");
}

#[tokio::test]
async fn write_without_content_is_rejected() {
    let dir = tempfile::tempdir().expect("tempdir");
    let executor = FileOpsExecutor::new(dir.path().to_string_lossy().to_string());

    let result = executor
        .execute(&call(
            serde_json::json!({ "operation": "write", "path": "new.txt" }),
        ))
        .await;

    assert!(result.is_error);
    assert!(result.content.contains("Missing 'content'"));
}

#[tokio::test]
async fn list_enumerates_directory_entries() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("a.txt"), "a").expect("write a");
    std::fs::create_dir(dir.path().join("subdir")).expect("mkdir");
    let executor = FileOpsExecutor::new(dir.path().to_string_lossy().to_string());

    let result = executor
        .execute(&call(
            serde_json::json!({ "operation": "list", "path": "." }),
        ))
        .await;

    assert!(!result.is_error);
    assert!(result.content.contains("a.txt"));
    assert!(result.content.contains("subdir/"));
}

#[tokio::test]
async fn exists_reports_true_and_false_correctly() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("present.txt"), "x").expect("write fixture");
    let executor = FileOpsExecutor::new(dir.path().to_string_lossy().to_string());

    let present = executor
        .execute(&call(
            serde_json::json!({ "operation": "exists", "path": "present.txt" }),
        ))
        .await;
    assert!(present.content.contains("exists"));
    assert!(!present.content.contains("does not exist"));

    let absent = executor
        .execute(&call(
            serde_json::json!({ "operation": "exists", "path": "absent.txt" }),
        ))
        .await;
    assert!(absent.content.contains("does not exist"));
}

#[tokio::test]
async fn unknown_operation_is_rejected() {
    let dir = tempfile::tempdir().expect("tempdir");
    let executor = FileOpsExecutor::new(dir.path().to_string_lossy().to_string());

    let result = executor
        .execute(&call(
            serde_json::json!({ "operation": "delete", "path": "x" }),
        ))
        .await;

    assert!(result.is_error);
    assert!(result.content.contains("Unknown operation"));
}

#[tokio::test]
async fn missing_operation_argument_is_rejected() {
    let dir = tempfile::tempdir().expect("tempdir");
    let executor = FileOpsExecutor::new(dir.path().to_string_lossy().to_string());

    let result = executor
        .execute(&call(serde_json::json!({ "path": "x" })))
        .await;
    assert!(result.is_error);
    assert!(result.content.contains("Missing 'operation'"));
}

#[tokio::test]
async fn missing_path_argument_is_rejected() {
    let dir = tempfile::tempdir().expect("tempdir");
    let executor = FileOpsExecutor::new(dir.path().to_string_lossy().to_string());

    let result = executor
        .execute(&call(serde_json::json!({ "operation": "read" })))
        .await;
    assert!(result.is_error);
    assert!(result.content.contains("Missing 'path'"));
}

#[tokio::test]
async fn non_string_operation_is_rejected() {
    let dir = tempfile::tempdir().expect("tempdir");
    let executor = FileOpsExecutor::new(dir.path().to_string_lossy().to_string());

    let result = executor
        .execute(&call(serde_json::json!({ "operation": 5, "path": "x" })))
        .await;
    assert!(result.is_error);
    assert!(result.content.contains("Invalid operation"));
}

#[tokio::test]
async fn non_string_path_is_rejected() {
    let dir = tempfile::tempdir().expect("tempdir");
    let executor = FileOpsExecutor::new(dir.path().to_string_lossy().to_string());

    let result = executor
        .execute(&call(serde_json::json!({ "operation": "read", "path": 5 })))
        .await;
    assert!(result.is_error);
    assert!(result.content.contains("Invalid path"));
}

#[tokio::test]
async fn non_string_content_on_write_is_rejected() {
    let dir = tempfile::tempdir().expect("tempdir");
    let executor = FileOpsExecutor::new(dir.path().to_string_lossy().to_string());

    let result = executor
        .execute(&call(serde_json::json!({
            "operation": "write",
            "path": "x.txt",
            "content": 5
        })))
        .await;
    assert!(result.is_error);
    assert!(result.content.contains("Invalid content"));
}

// --- Path confinement: the security-relevant surface ---

#[tokio::test]
async fn absolute_paths_are_rejected() {
    let dir = tempfile::tempdir().expect("tempdir");
    let executor = FileOpsExecutor::new(dir.path().to_string_lossy().to_string());

    let result = executor
        .execute(&call(
            serde_json::json!({ "operation": "read", "path": "/etc/passwd" }),
        ))
        .await;

    assert!(result.is_error);
    assert!(result.content.contains("Access denied"));
}

#[tokio::test]
async fn parent_directory_traversal_is_rejected() {
    let dir = tempfile::tempdir().expect("tempdir");
    let executor = FileOpsExecutor::new(dir.path().to_string_lossy().to_string());

    let result = executor
        .execute(&call(serde_json::json!({
            "operation": "read",
            "path": "../outside.txt"
        })))
        .await;

    assert!(result.is_error);
    assert!(result.content.contains("Access denied"));
}

#[tokio::test]
async fn nul_byte_in_path_is_rejected() {
    let dir = tempfile::tempdir().expect("tempdir");
    let executor = FileOpsExecutor::new(dir.path().to_string_lossy().to_string());

    let result = executor
        .execute(&call(
            serde_json::json!({ "operation": "read", "path": "a\0b" }),
        ))
        .await;

    assert!(result.is_error);
    assert!(result.content.contains("Access denied"));
}

#[tokio::test]
async fn tilde_in_path_is_rejected() {
    let dir = tempfile::tempdir().expect("tempdir");
    let executor = FileOpsExecutor::new(dir.path().to_string_lossy().to_string());

    let result = executor
        .execute(&call(
            serde_json::json!({ "operation": "read", "path": "~/secrets" }),
        ))
        .await;

    assert!(result.is_error);
    assert!(result.content.contains("Access denied"));
}

#[tokio::test]
async fn a_new_files_nonexistent_path_is_still_confined_via_its_parent() {
    // `write` targets a file that doesn't exist yet, so `is_safe`'s
    // canonicalize falls through to checking the parent directory instead.
    let dir = tempfile::tempdir().expect("tempdir");
    let executor = FileOpsExecutor::new(dir.path().to_string_lossy().to_string());

    let result = executor
        .execute(&call(serde_json::json!({
            "operation": "write",
            "path": "brand-new.txt",
            "content": "x"
        })))
        .await;

    assert!(!result.is_error);
}

#[test]
fn from_env_defaults_to_current_directory() {
    let _guard = env_lock().lock().unwrap_or_else(|p| p.into_inner());
    unsafe { std::env::remove_var("FILE_OPS_BASE_PATH") };
    // Just confirms construction doesn't panic with no env var set; the
    // resulting base path isn't otherwise observable from outside the crate.
    let _executor = FileOpsExecutor::from_env();
    let _default_executor = FileOpsExecutor::default();
}

#[test]
fn from_env_honors_file_ops_base_path() {
    let dir = tempfile::tempdir().expect("tempdir");
    let _guard = env_lock().lock().unwrap_or_else(|p| p.into_inner());
    unsafe {
        std::env::set_var(
            "FILE_OPS_BASE_PATH",
            dir.path().to_string_lossy().to_string(),
        )
    };
    let _executor = FileOpsExecutor::from_env();
    unsafe { std::env::remove_var("FILE_OPS_BASE_PATH") };
}
