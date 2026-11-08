//! A book written to a temporary directory for a test.

use quench_core::tmp::TempDir;

/// Writes `files` (path, content) under a fresh directory and returns it.
pub fn book(files: &[(&str, &str)]) -> TempDir {
    let dir = TempDir::new().unwrap();
    for (path, content) in files {
        let full = dir.path().join(path);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(full, content).unwrap();
    }
    dir
}

/// A page with front matter `head` (YAML lines) and `body`.
pub fn page(head: &str, body: &str) -> String {
    format!("---\n{head}\n---\n{body}")
}
