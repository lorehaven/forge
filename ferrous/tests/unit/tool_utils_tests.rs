use ferrous::tools::utils::{clean_path, resolve_dir, resolve_parent_for_write};
use std::env;
use std::fs;

#[test]
fn test_clean_path() {
    assert_eq!(clean_path("  path/to/file  "), "path/to/file");
    assert_eq!(clean_path("\"path/with/quotes\""), "path/with/quotes");
    assert_eq!(clean_path("  \"  path/with/both  \"  "), "path/with/both");
}

#[test]
fn test_resolve_dir() {
    let cwd = env::current_dir().unwrap();

    // Resolve current dir
    let resolved = resolve_dir(&cwd, ".").unwrap();
    assert_eq!(resolved, cwd.canonicalize().unwrap());

    // Resolve subdirectory
    let resolved = resolve_dir(&cwd, "src").unwrap();
    assert!(resolved.ends_with("src"));

    // Path traversal attempt
    let result = resolve_dir(&cwd, "..");
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().to_string(), "Path traversal attempt");
}

#[test]
fn test_resolve_parent_for_write() {
    let cwd = env::current_dir().unwrap();
    let cwd_canonical = cwd.canonicalize().unwrap();

    // File in current dir
    let resolved = resolve_parent_for_write(&cwd, "test.txt").unwrap();
    assert_eq!(resolved, cwd_canonical.join("test.txt"));

    // File in subdir
    let resolved = resolve_parent_for_write(&cwd, "src/test.txt").unwrap();
    assert_eq!(resolved, cwd_canonical.join("src/test.txt"));

    // Path traversal attempt in parent
    let result = resolve_parent_for_write(&cwd, "../test.txt");
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().to_string(), "Path traversal attempt");
}

#[test]
fn test_resolve_dir_workspace_module_fallback() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    fs::create_dir_all(root.join("ferrous/src")).unwrap();
    fs::write(root.join("ferrous/src/agent.rs"), "pub fn x() {}").unwrap();

    let resolved = resolve_dir(root, "src/agent.rs").unwrap();
    assert_eq!(
        resolved,
        root.join("ferrous/src/agent.rs").canonicalize().unwrap()
    );
}
