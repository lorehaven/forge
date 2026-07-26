use riveter::render::strip_empty_lines;

#[test]
fn test_strip_empty_lines() {
    let input = "line1\n\nline2\n  \nline3\n";
    let expected = "line1\nline2\nline3\n";
    assert_eq!(strip_empty_lines(input), expected);

    let input = "\n\n  \n";
    let expected = "\n";
    assert_eq!(strip_empty_lines(input), expected);
}

use riveter::render::{ResourceScope, resource_in_scope};
use serde_yaml::Value as YamlValue;

fn parse_resource(yaml: &str) -> YamlValue {
    serde_yaml::from_str(yaml).expect("resource yaml should parse")
}

#[test]
fn immutable_flag_marks_resource_immutable() {
    let res = parse_resource("kind: namespace\nimmutable: true\n");
    assert!(!resource_in_scope(&res, ResourceScope::Mutable));
    assert!(resource_in_scope(&res, ResourceScope::Immutable));
}

#[test]
fn lifecycle_static_marks_resource_immutable() {
    let res = parse_resource("kind: ingress\nlifecycle: static\n");
    assert!(!resource_in_scope(&res, ResourceScope::Mutable));
    assert!(resource_in_scope(&res, ResourceScope::Immutable));
}

#[test]
fn default_resources_are_mutable() {
    let res = parse_resource("kind: deployment\n");
    assert!(resource_in_scope(&res, ResourceScope::Mutable));
    assert!(!resource_in_scope(&res, ResourceScope::Immutable));
}
