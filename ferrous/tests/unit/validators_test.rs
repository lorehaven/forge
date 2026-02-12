use ferrous::tools::validators::{
    validate_is_directory, validate_is_file, validate_path_exists, validate_search_exists,
};
use std::fs;
use std::io::Write;
use tempfile::TempDir;

#[test]
fn test_validate_search_exists_success() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.txt");

    let mut file = fs::File::create(&file_path).unwrap();
    writeln!(file, "Hello, world!").unwrap();
    writeln!(file, "This is a test.").unwrap();

    // Should succeed - search string exists
    let result = validate_search_exists(&file_path, "Hello, world!");
    assert!(result.is_ok());
}

#[test]
fn test_validate_search_exists_failure() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.txt");

    let mut file = fs::File::create(&file_path).unwrap();
    writeln!(file, "Hello, world!").unwrap();

    // Should fail - search string doesn't exist
    let result = validate_search_exists(&file_path, "Goodbye");
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Search string not found")
    );
}

#[test]
fn test_validate_search_exists_file_not_found() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("nonexistent.txt");

    // Should fail - file doesn't exist
    let result = validate_search_exists(&file_path, "anything");
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("does not exist"));
}

#[test]
fn test_validate_path_exists() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.txt");

    fs::File::create(&file_path).unwrap();

    // Should succeed - path exists
    assert!(validate_path_exists(&file_path).is_ok());

    // Should fail - path doesn't exist
    let nonexistent = temp_dir.path().join("nonexistent.txt");
    assert!(validate_path_exists(&nonexistent).is_err());
}

#[test]
fn test_validate_is_file() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.txt");

    fs::File::create(&file_path).unwrap();

    // Should succeed - is a file
    assert!(validate_is_file(&file_path).is_ok());

    // Should fail - is a directory
    assert!(validate_is_file(temp_dir.path()).is_err());
}

#[test]
fn test_validate_is_directory() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.txt");

    fs::File::create(&file_path).unwrap();

    // Should succeed - is a directory
    assert!(validate_is_directory(temp_dir.path()).is_ok());

    // Should fail - is a file
    assert!(validate_is_directory(&file_path).is_err());
}

#[test]
fn test_validate_search_exact_whitespace_match() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.txt");

    let mut file = fs::File::create(&file_path).unwrap();
    writeln!(file, "    indented text").unwrap();
    writeln!(file, "not indented").unwrap();

    // Should succeed - exact match with whitespace
    assert!(validate_search_exists(&file_path, "    indented text").is_ok());

    // Should succeed - substring exists (this is how .contains() works in Rust)
    // The search validates that the substring exists, not exact line matching
    assert!(validate_search_exists(&file_path, "indented text").is_ok());

    // Should fail - this string doesn't exist at all
    assert!(validate_search_exists(&file_path, "missing string").is_err());
}
