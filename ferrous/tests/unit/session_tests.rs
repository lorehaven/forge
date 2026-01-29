use ferrous::core::sessions::{sanitize_name, generate_filename};
use uuid::Uuid;

#[test]
fn test_sanitize_name() {
    assert_eq!(sanitize_name("  Hello World  "), "hello-world");
    assert_eq!(sanitize_name("Feature: New Tool!"), "feature-new-tool");
    assert_eq!(sanitize_name("special@#$%characters"), "special-characters");
    assert_eq!(sanitize_name("multiple---dashes"), "multiple-dashes");
    assert_eq!(sanitize_name("   "), "");
}

#[test]
fn test_generate_filename() {
    let id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
    let name = "Test Session";
    let filename = generate_filename(name, id);
    
    // Format: {timestamp}_{safe_name}_{short_id}.json
    assert!(filename.contains("_test-session_550e8400.json"));
    
    let unnamed_filename = generate_filename("", id);
    assert!(unnamed_filename.contains("_unnamed-"));
    assert!(unnamed_filename.contains("_550e8400.json"));
}
