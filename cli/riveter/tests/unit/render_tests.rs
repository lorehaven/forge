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

use riveter::render::{Selector, check_embedded_templates, template_name_for_kind};

fn selector(targets: &[&str]) -> Selector {
    Selector::parse(targets).expect("selector should parse")
}

#[test]
fn empty_selector_matches_everything() {
    let sel = Selector::default();
    assert!(sel.is_empty());
    assert!(sel.matches("deployment", "api"));
    assert!(sel.matches("secret", "anything"));
}

#[test]
fn kind_and_name_select_one_resource() {
    let sel = selector(&["deployment/api"]);
    assert!(sel.matches("deployment", "api"));
    assert!(!sel.matches("deployment", "web"));
    assert!(!sel.matches("service", "api"));
}

#[test]
fn bare_kind_selects_every_resource_of_that_kind() {
    let sel = selector(&["deployment"]);
    assert!(sel.matches("deployment", "api"));
    assert!(sel.matches("deployment", "web"));
    assert!(!sel.matches("service", "api"));
}

#[test]
fn wildcards_apply_to_both_halves() {
    assert!(selector(&["*/api"]).matches("service", "api"));
    assert!(selector(&["deployment/*"]).matches("deployment", "anything"));
    assert!(selector(&["*/api-*"]).matches("service", "api-internal"));
    assert!(!selector(&["*/api-*"]).matches("service", "web"));
    assert!(selector(&["deployment/ap?"]).matches("deployment", "api"));
    assert!(!selector(&["deployment/ap?"]).matches("deployment", "apiv2"));
}

#[test]
fn selector_is_case_insensitive_and_alias_aware() {
    assert!(selector(&["StatefulSet/pg"]).matches("statefulset", "pg"));
    assert!(selector(&["sts/pg"]).matches("statefulset", "pg"));
    assert!(selector(&["hpa"]).matches("horizontalpodautoscaler", "api"));
    // the overlay may also spell the kind with the alias
    assert!(selector(&["horizontalpodautoscaler"]).matches("hpa", "api"));
}

#[test]
fn multiple_targets_union() {
    let sel = selector(&["deployment/api", "service/api"]);
    assert!(sel.matches("deployment", "api"));
    assert!(sel.matches("service", "api"));
    assert!(!sel.matches("configmap", "api"));
}

#[test]
fn malformed_targets_are_rejected() {
    assert!(Selector::parse(&["deployment/api/extra"]).is_err());
    assert!(Selector::parse(&["   "]).is_err());
}

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
