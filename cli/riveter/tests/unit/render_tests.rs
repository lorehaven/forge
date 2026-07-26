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

use riveter::render::{check_embedded_templates, template_name_for_kind};

#[test]
fn every_embedded_template_parses() {
    let names = check_embedded_templates().expect("all embedded templates should parse");
    assert!(names.contains(&"deployment.yaml.j2"));
    assert!(names.contains(&"_macros.yaml.j2"));
}

#[test]
fn kind_lookup_is_case_insensitive() {
    assert_eq!(
        template_name_for_kind("StatefulSet"),
        "statefulset.yaml.j2"
    );
    assert_eq!(template_name_for_kind("secret"), "secret.yaml.j2");
}

#[test]
fn kind_aliases_resolve_to_canonical_templates() {
    let names = check_embedded_templates().expect("templates should parse");

    for (alias, expected) in [
        ("hpa", "horizontalpodautoscaler.yaml.j2"),
        ("pdb", "poddisruptionbudget.yaml.j2"),
        ("crd", "customresourcedefinition.yaml.j2"),
        ("netpol", "networkpolicy.yaml.j2"),
        ("PersistentVolumeClaim", "pvc.yaml.j2"),
    ] {
        let resolved = template_name_for_kind(alias);
        assert_eq!(resolved, expected, "alias `{alias}`");
        assert!(names.contains(&expected), "`{expected}` is not registered");
    }
}
