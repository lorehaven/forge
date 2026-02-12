use ferrous::core::Indexer;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_index_and_search() {
    let dir = tempdir().unwrap();
    let index_path = dir.path().join("index");
    let project_path = dir.path().join("project");
    fs::create_dir_all(&project_path).unwrap();

    let file1 = project_path.join("file1.rs");
    fs::write(&file1, "fn main() { println!(\"hello world\"); }").unwrap();

    let file2 = project_path.join("file2.md");
    fs::write(&file2, "# This is a test\nSome content here.").unwrap();

    let indexer = Indexer::new(&index_path).unwrap();
    indexer.index_project(&project_path).unwrap();

    // Search for content in file1
    let results = indexer.search("hello", 10).unwrap();
    assert!(!results.is_empty());
    assert_eq!(results[0].0, "file1.rs");
    assert!(results[0].1.contains("hello world"));

    // Search for content in file2
    let results = indexer.search("test", 10).unwrap();
    assert!(!results.is_empty());
    assert_eq!(results[0].0, "file2.md");
    assert!(results[0].1.contains("This is a test"));

    // Update file1 and re-index
    fs::write(&file1, "fn main() { println!(\"updated content\"); }").unwrap();
    indexer.index_project(&project_path).unwrap();

    let results = indexer.search("updated", 10).unwrap();
    assert!(!results.is_empty());
    assert_eq!(results[0].0, "file1.rs");
    assert!(results[0].1.contains("updated content"));

    // Old content should be gone (after re-index)
    let results = indexer.search("hello", 10).unwrap();
    assert!(
        results.is_empty() || results[0].0 != "file1.rs" || !results[0].1.contains("hello world")
    );
}
