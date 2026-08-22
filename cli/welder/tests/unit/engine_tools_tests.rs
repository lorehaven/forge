// `with_temp_cwd`'s returned guard is deliberately held for a whole test
// body (it also restores the original cwd on drop), so there's no tighter
// scope clippy's `significant_drop_tightening` could suggest without
// breaking the fixture.
#![allow(clippy::significant_drop_tightening)]

use crate::support;

use serde_json::json;
use welder::engine::tools::{is_command_allowed, run_tool, safe_rel_path, tool_help};

// -----------------------------------------------------------------
// safe_rel_path
// -----------------------------------------------------------------

#[test]
fn rejects_absolute_paths() {
    assert!(safe_rel_path("/etc/passwd").is_err());
}

#[test]
fn rejects_parent_dir_traversal() {
    assert!(safe_rel_path("../secrets").is_err());
    assert!(safe_rel_path("a/../../b").is_err());
}

#[test]
fn allows_plain_relative_paths() {
    let _guard = support::cwd_lock().lock().unwrap();
    assert!(safe_rel_path("src/main.rs").is_ok());
    assert!(safe_rel_path(".").is_ok());
}

#[test]
fn allows_paths_that_do_not_exist_yet() {
    let _guard = support::cwd_lock().lock().unwrap();
    // A brand-new file under a directory that doesn't exist yet either
    // (write_file creates parents) must still be allowed.
    assert!(safe_rel_path("brand/new/file.txt").is_ok());
}

#[test]
#[cfg(unix)]
fn rejects_symlink_escape() {
    let _guard = support::cwd_lock().lock().unwrap();

    let tmp = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("secret.txt"), "hi").unwrap();
    std::os::unix::fs::symlink(outside.path(), tmp.path().join("escape")).unwrap();

    let original_cwd = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();
    let result = safe_rel_path("escape/secret.txt");
    std::env::set_current_dir(original_cwd).unwrap();

    assert!(result.is_err(), "symlink escape should be rejected");
}

// -----------------------------------------------------------------
// is_command_allowed
// -----------------------------------------------------------------

#[test]
fn allowlist_matches_by_whitespace_tokenized_prefix() {
    let allowlist = vec!["cargo check".to_string(), "npm test".to_string()];

    let allowed: Vec<String> = ["cargo", "check", "-p", "welder"]
        .into_iter()
        .map(String::from)
        .collect();
    assert!(is_command_allowed(&allowed, &allowlist));

    let disallowed: Vec<String> = ["cargo", "publish"].into_iter().map(String::from).collect();
    assert!(!is_command_allowed(&disallowed, &allowlist));
}

#[test]
fn allowlist_rejects_shorter_command_than_pattern() {
    let allowlist = vec!["cargo check".to_string()];
    let short: Vec<String> = vec!["cargo".to_string()];
    assert!(!is_command_allowed(&short, &allowlist));
}

#[test]
fn allowlist_empty_matches_nothing() {
    let cmd: Vec<String> = ["cargo", "check"].into_iter().map(String::from).collect();
    assert!(!is_command_allowed(&cmd, &[]));
}

// -----------------------------------------------------------------
// tool_help
// -----------------------------------------------------------------

#[test]
fn tool_help_documents_every_known_tool() {
    let tools = vec![
        "list_dir".to_string(),
        "read_file".to_string(),
        "write_file".to_string(),
        "replace_in_file".to_string(),
        "search".to_string(),
        "index_project".to_string(),
        "run_cmd".to_string(),
    ];
    let help = tool_help(&tools, &["cargo check".to_string()]);
    assert!(help.contains("list_dir"));
    assert!(help.contains("read_file"));
    assert!(help.contains("write_file"));
    assert!(help.contains("replace_in_file"));
    assert!(help.contains("search"));
    assert!(help.contains("index_project"));
    assert!(help.contains("run_cmd"));
    assert!(help.contains("cargo check"));
}

#[test]
fn tool_help_skips_an_unknown_tool_and_omits_the_allowlist_line_without_run_cmd() {
    let tools = vec!["list_dir".to_string(), "not-a-real-tool".to_string()];
    let help = tool_help(&tools, &[]);
    assert!(help.contains("list_dir"));
    assert!(!help.contains("allowlist"));
}

// -----------------------------------------------------------------
// run_tool - dispatches into list_dir/read_file/write_file/replace_in_file/
// search/index_project/run_cmd, all private, so this is the cheapest way to
// exercise them without bumping every one to `pub` individually.
// -----------------------------------------------------------------

struct WithTempCwd {
    _guard: std::sync::MutexGuard<'static, ()>,
    original: std::path::PathBuf,
    dir: tempfile::TempDir,
}

fn with_temp_cwd() -> WithTempCwd {
    let guard = support::cwd_lock().lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let original = std::env::current_dir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();
    WithTempCwd {
        _guard: guard,
        original,
        dir,
    }
}

impl Drop for WithTempCwd {
    fn drop(&mut self) {
        std::env::set_current_dir(&self.original).unwrap();
    }
}

#[test]
fn run_tool_unknown_tool_errors() {
    let err = run_tool("does_not_exist", &json!({}), &[]).unwrap_err();
    assert!(err.to_string().contains("unknown tool"));
}

#[test]
fn run_tool_list_dir_lists_files_and_dirs_sorted() {
    let cwd = with_temp_cwd();
    std::fs::write(cwd.dir.path().join("b.txt"), "b").unwrap();
    std::fs::create_dir(cwd.dir.path().join("a-dir")).unwrap();

    let result = run_tool("list_dir", &json!({"path": "."}), &[]).unwrap();
    assert!(result.output.contains("file\tb.txt"));
    assert!(result.output.contains("dir\ta-dir"));
}

#[test]
fn run_tool_read_file_returns_line_numbered_content() {
    let cwd = with_temp_cwd();
    std::fs::write(cwd.dir.path().join("f.txt"), "one\ntwo\nthree\n").unwrap();

    let result = run_tool(
        "read_file",
        &json!({"path": "f.txt", "start_line": 2, "end_line": 3}),
        &[],
    )
    .unwrap();
    assert!(result.output.contains("2 | two"));
    assert!(result.output.contains("3 | three"));
    assert!(!result.output.contains("one"));
}

#[test]
fn run_tool_read_file_rejects_an_invalid_line_range() {
    let cwd = with_temp_cwd();
    std::fs::write(cwd.dir.path().join("f.txt"), "one\n").unwrap();

    let err = run_tool(
        "read_file",
        &json!({"path": "f.txt", "start_line": 5, "end_line": 1}),
        &[],
    )
    .unwrap_err();
    assert!(err.to_string().contains("invalid line range"));
}

#[test]
fn run_tool_write_file_creates_parent_directories_and_writes_content() {
    let cwd = with_temp_cwd();
    let result = run_tool(
        "write_file",
        &json!({"path": "nested/dir/f.txt", "content": "hello"}),
        &[],
    )
    .unwrap();
    assert!(result.output.contains("wrote"));
    assert_eq!(
        std::fs::read_to_string(cwd.dir.path().join("nested/dir/f.txt")).unwrap(),
        "hello"
    );
}

#[test]
fn run_tool_replace_in_file_counts_and_applies_the_replacement() {
    let cwd = with_temp_cwd();
    std::fs::write(cwd.dir.path().join("f.txt"), "foo bar foo").unwrap();

    let result = run_tool(
        "replace_in_file",
        &json!({"path": "f.txt", "find": "foo", "replace": "baz"}),
        &[],
    )
    .unwrap();
    assert!(result.output.contains("replaced 2 occurrence"));
    assert_eq!(
        std::fs::read_to_string(cwd.dir.path().join("f.txt")).unwrap(),
        "baz bar baz"
    );
}

#[test]
fn run_tool_search_finds_matching_lines() {
    let cwd = with_temp_cwd();
    std::fs::write(cwd.dir.path().join("f.txt"), "hello world\nno match here\n").unwrap();

    let result = run_tool("search", &json!({"pattern": "hello", "path": "."}), &[]).unwrap();
    assert!(result.output.contains("hello world"));
    assert!(!result.output.contains("no match here"));
}

#[test]
fn run_tool_search_rejects_an_invalid_pattern() {
    let _cwd = with_temp_cwd();
    let err = run_tool("search", &json!({"pattern": "("}), &[]).unwrap_err();
    assert!(err.to_string().contains("invalid pattern"));
}

#[test]
fn run_tool_index_project_lists_every_file() {
    let cwd = with_temp_cwd();
    std::fs::write(cwd.dir.path().join("a.txt"), "a").unwrap();
    std::fs::create_dir(cwd.dir.path().join("sub")).unwrap();
    std::fs::write(cwd.dir.path().join("sub/b.txt"), "b").unwrap();

    let result = run_tool("index_project", &json!({"path": "."}), &[]).unwrap();
    assert!(result.output.contains("a.txt"));
    assert!(result.output.contains("b.txt"));
}

#[test]
fn run_tool_run_cmd_rejects_a_command_missing_from_the_allowlist() {
    let _cwd = with_temp_cwd();
    let err = run_tool("run_cmd", &json!({"cmd": "rm -rf /"}), &[]).unwrap_err();
    assert!(err.to_string().contains("blocked by run_cmd allowlist"));
}

#[test]
fn run_tool_run_cmd_rejects_a_path_qualified_executable() {
    let _cwd = with_temp_cwd();
    let err = run_tool(
        "run_cmd",
        &json!({"cmd": "/bin/echo hi"}),
        &["/bin/echo".to_string()],
    )
    .unwrap_err();
    assert!(err.to_string().contains("only bare executable names"));
}

#[test]
fn run_tool_run_cmd_runs_an_allowlisted_command_for_real() {
    // `echo` is about as safe a real subprocess as this gets - no
    // filesystem/network side effects, and it's on every machine that can
    // build this crate.
    let _cwd = with_temp_cwd();
    let result = run_tool(
        "run_cmd",
        &json!({"cmd": "echo hello"}),
        &["echo".to_string()],
    )
    .unwrap();
    assert!(result.output.contains("hello"));
    assert!(result.output.contains("status:"));
}
