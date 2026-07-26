//! Unit tests for `tools/web_search.rs`.

use sage_service::tools::web_search::*;

#[test]
fn test_web_search_definition() {
    let def = get_definition();
    assert_eq!(def.name, "web_search");
    assert!(!def.description.is_empty());
    assert_eq!(def.parameters.param_type, "object");
    assert!(def.parameters.required.contains(&"query".to_string()));
}
