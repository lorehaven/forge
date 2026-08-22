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

#[test]
fn blank_lines_inside_a_block_scalar_are_content() {
    // Removing this blank line rewrites the file the pod mounts.
    let input = "data:\n  motd: |\n    line one\n\n    line three\nnext: value\n";
    assert_eq!(strip_empty_lines(input), input);
}

#[test]
fn blank_lines_after_a_block_scalar_are_still_stripped() {
    let input = "data:\n  motd: |\n    only line\n\nnext: value\n";
    let expected = "data:\n  motd: |\n    only line\nnext: value\n";
    assert_eq!(strip_empty_lines(input), expected);
}

#[test]
fn every_block_scalar_spelling_is_recognised() {
    for header in ["|", "|-", "|+", ">", ">-", ">+", "|2", "|2-"] {
        let input = format!("  key: {header}\n    a\n\n    b\n");
        assert_eq!(strip_empty_lines(&input), input, "header `{header}`");
    }
}

#[test]
fn a_block_scalar_under_a_sequence_item_is_recognised() {
    let input = "items:\n  - |\n    a\n\n    b\n";
    assert_eq!(strip_empty_lines(input), input);
}

#[test]
fn consecutive_block_scalars_are_each_tracked() {
    let input = "a: |\n  one\n\n  two\nb: |\n  three\n\n  four\n";
    assert_eq!(strip_empty_lines(input), input);
}

#[test]
fn a_pipe_inside_a_plain_value_does_not_open_a_scalar() {
    // `a|b` is data, not a block header — the blank line after it is template
    // whitespace and should still go.
    let input = "cmd: sh -c a|b\n\nnext: value\n";
    let expected = "cmd: sh -c a|b\nnext: value\n";
    assert_eq!(strip_empty_lines(input), expected);

    let input = "args: [\"sh\", \"-c\", \"a|b\"]\n\nnext: value\n";
    let expected = "args: [\"sh\", \"-c\", \"a|b\"]\nnext: value\n";
    assert_eq!(strip_empty_lines(input), expected);
}

#[test]
fn keep_chomping_preserves_the_blank_run_that_trails_a_scalar() {
    // `to_yaml` emits `|+` for a string ending in more than one newline, so a
    // `raw` block can carry one. Under `|+` those blanks are the value.
    let input = "data:\n  script: |+\n    line1\n\n\nkind: ConfigMap\n";
    assert_eq!(strip_empty_lines(input), input);

    // ... and at the end of a document, with no key following.
    let tail = "data:\n  script: |+\n    line1\n\n\n";
    assert_eq!(strip_empty_lines(tail), tail);
}

#[test]
fn clip_and_strip_chomping_still_drop_the_trailing_blank_run() {
    // Under `|` and `|-` YAML discards those newlines anyway, so keeping them
    // would just leave template whitespace in the manifest.
    for header in ["|", "|-", ">", ">-"] {
        let input = format!("data:\n  script: {header}\n    line1\n\n\nkind: ConfigMap\n");
        let expected = format!("data:\n  script: {header}\n    line1\nkind: ConfigMap\n");
        assert_eq!(strip_empty_lines(&input), expected, "header `{header}`");
    }
}

#[test]
fn a_scalar_ending_the_document_keeps_its_interior_blanks() {
    let input = "data:\n  motd: |\n    one\n\n    two\n";
    assert_eq!(strip_empty_lines(input), input);
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
    assert_eq!(template_name_for_kind("StatefulSet"), "statefulset.yaml.j2");
    assert_eq!(template_name_for_kind("secret"), "secret.yaml.j2");
}

use riveter::render::substitute_vars;
use std::collections::HashMap;

fn vars(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect()
}

#[test]
fn defined_variables_are_expanded() {
    let mut data = parse_resource("kind: secret\nstring_data:\n  password: ${DB_PASSWORD}\n");
    substitute_vars(&mut data, &vars(&[("DB_PASSWORD", "hunter2")]), None)
        .expect("defined variable should expand");

    assert_eq!(data["string_data"]["password"].as_str(), Some("hunter2"));
}

#[test]
fn undefined_variables_fail_the_render() {
    // Passing these through would ship the literal `${...}` to the cluster —
    // as a Secret value, that is the placeholder becoming the password.
    let mut data = parse_resource("kind: secret\nstring_data:\n  password: ${NOPE}\n");
    let err = substitute_vars(&mut data, &vars(&[]), None).expect_err("should fail");

    let msg = err.to_string();
    assert!(msg.contains("${NOPE}"), "{msg}");
    assert!(msg.contains("string_data.password"), "{msg}");
}

#[test]
fn the_error_names_every_missing_variable_and_the_env_file() {
    let mut data = parse_resource("kind: raw\na: ${ONE}\nb: ${TWO}\nc: ${ONE}\n");
    let err = substitute_vars(&mut data, &vars(&[]), Some("overlays/prod/.env"))
        .expect_err("should fail");

    let msg = err.to_string();
    assert!(msg.contains("${ONE}"), "{msg}");
    assert!(msg.contains("${TWO}"), "{msg}");
    assert!(msg.contains("overlays/prod/.env"), "{msg}");
    // `ONE` appears twice in the overlay but should be reported once.
    assert_eq!(msg.matches("${ONE}").count(), 1, "{msg}");
}

#[test]
fn paths_in_the_error_point_into_sequences() {
    let mut data = parse_resource("resources:\n  - kind: pod\n    image: ${IMG}\n");
    let err = substitute_vars(&mut data, &vars(&[]), None).expect_err("should fail");

    assert!(err.to_string().contains("resources[0].image"), "{err}");
}

#[test]
fn a_doubled_dollar_escapes_to_a_literal_reference() {
    // For a value some later stage expands — a shell in a container command.
    let mut data = parse_resource("kind: pod\nargs:\n  - echo $${HOME}\n");
    substitute_vars(&mut data, &vars(&[]), None).expect("escaped literal should not fail");

    assert_eq!(data["args"][0].as_str(), Some("echo ${HOME}"));
}

#[test]
fn an_escaped_reference_is_not_expanded_even_when_defined() {
    let mut data = parse_resource("kind: pod\nargs:\n  - $${HOME}\n");
    substitute_vars(&mut data, &vars(&[("HOME", "/root")]), None).expect("should not fail");

    assert_eq!(data["args"][0].as_str(), Some("${HOME}"));
}

#[test]
fn substitution_reaches_nested_maps_and_sequences() {
    let mut data = parse_resource(
        "resources:\n  - kind: deployment\n    env_vars:\n      URL: https://${HOST}/api\n",
    );
    substitute_vars(&mut data, &vars(&[("HOST", "example.com")]), None).expect("should expand");

    assert_eq!(
        data["resources"][0]["env_vars"]["URL"].as_str(),
        Some("https://example.com/api")
    );
}

#[test]
fn the_missing_variable_error_names_the_environments_own_env_file() {
    // There is no fallback to a shared `.env`: one would let an environment
    // resolve a variable from another environment's file. The error has to say
    // which file should define it.
    let mut data = parse_resource("kind: secret\nstring_data:\n  password: ${NOPE}\n");
    let err = substitute_vars(&mut data, &vars(&[]), Some("overlays/prod/.env"))
        .expect_err("should fail");

    assert!(err.to_string().contains("overlays/prod/.env"), "{err}");
}

use riveter::render::{kinds_match, prunable_kinds};

#[test]
fn prune_matches_the_plural_kinds_kubectl_prints() {
    // `kubectl -o name` prints plurals, so an overlay kind has to line up with
    // them — a mismatch here would report a live resource as orphaned and
    // delete it.
    for (overlay, live) in [
        ("deployment", "deployments"),
        ("service", "services"),
        ("ingress", "ingresses"),
        ("statefulset", "statefulsets"),
        ("pvc", "persistentvolumeclaims"),
        ("sts", "statefulsets"),
        ("networkpolicy", "networkpolicies"),
        ("configmap", "configmaps"),
    ] {
        assert!(kinds_match(overlay, live), "`{overlay}` vs `{live}`");
    }
}

#[test]
fn prune_does_not_confuse_different_kinds() {
    for (overlay, live) in [
        ("deployment", "services"),
        ("secret", "configmaps"),
        ("job", "cronjobs"),
        ("pv", "persistentvolumeclaims"),
    ] {
        assert!(!kinds_match(overlay, live), "`{overlay}` vs `{live}`");
    }
}

#[test]
fn prune_never_asks_about_namespaces_or_raw() {
    let kinds = prunable_kinds();

    // Deleting a namespace takes everything inside it; `raw` carries whatever
    // labels the overlay wrote, so riveter cannot claim ownership of it.
    assert!(!kinds.contains(&"namespace"), "namespace is prunable");
    assert!(!kinds.contains(&"raw"), "raw is prunable");
    assert!(!kinds.iter().any(|k| k.starts_with('_')), "macro leaked in");

    assert!(kinds.contains(&"deployment"));
    assert!(kinds.contains(&"configmap"));
}

use riveter::render::is_secret_resource;

#[test]
fn a_secret_resource_is_recognised_by_kind() {
    assert!(is_secret_resource("secret", "kind: Secret\n"));
    assert!(is_secret_resource("Secret", "kind: Secret\n"));
    assert!(!is_secret_resource("configmap", "kind: ConfigMap\n"));
    assert!(!is_secret_resource("deployment", "kind: Deployment\n"));
}

#[test]
fn a_secret_smuggled_through_raw_is_still_recognised() {
    // `raw` emits its manifest verbatim, so the kind alone does not say whether
    // the file ends up holding secret data.
    let yaml = "apiVersion: v1\nkind: Secret\nstringData:\n  token: s3cr3t\n";
    assert!(is_secret_resource("raw", yaml));

    let harmless = "apiVersion: v1\nkind: ConfigMap\ndata:\n  a: '1'\n";
    assert!(!is_secret_resource("raw", harmless));
}

#[test]
fn a_secret_mentioned_in_passing_does_not_count() {
    // A value that merely names Secret is not a Secret manifest.
    let yaml = "kind: Deployment\nannotations:\n  note: \"kind: Secret is not here\"\n";
    assert!(!is_secret_resource("deployment", yaml));
}

use riveter::render::kube_context;

#[test]
fn an_overlay_can_bind_itself_to_a_kubectl_context() {
    let data = parse_resource("kube_context: prod-cluster\nresources: []\n");
    assert_eq!(
        kube_context(&data).expect("should parse"),
        Some("prod-cluster".to_string())
    );
}

#[test]
fn an_overlay_without_a_context_is_unpinned() {
    let data = parse_resource("namespace_name: prod\nresources: []\n");
    assert_eq!(kube_context(&data).expect("should parse"), None);
}

#[test]
fn a_context_is_trimmed() {
    let data = parse_resource("kube_context: \"  prod-cluster  \"\n");
    assert_eq!(
        kube_context(&data).expect("should parse"),
        Some("prod-cluster".to_string())
    );
}

#[test]
fn a_blank_or_non_string_context_is_rejected() {
    // Silently treating these as "unpinned" would drop the binding the overlay
    // was clearly trying to express.
    for yaml in ["kube_context: \"\"\n", "kube_context: \"   \"\n"] {
        let data = parse_resource(yaml);
        assert!(kube_context(&data).is_err(), "`{yaml}`");
    }

    let data = parse_resource("kube_context: 42\n");
    let err = kube_context(&data).expect_err("a number is not a context");
    assert!(err.to_string().contains("must be a string"), "{err}");
}

#[test]
fn a_context_can_come_from_a_variable() {
    let mut data = parse_resource("kube_context: ${CLUSTER}\nresources: []\n");
    substitute_vars(&mut data, &vars(&[("CLUSTER", "prod-eu")]), None).expect("should expand");

    assert_eq!(
        kube_context(&data).expect("should parse"),
        Some("prod-eu".to_string())
    );
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

use riveter::render::ResourceRef;

fn resource_ref(kind: &str, name: &str, immutable: bool) -> ResourceRef {
    ResourceRef {
        kind: kind.to_string(),
        name: name.to_string(),
        immutable,
    }
}

#[test]
fn resource_scope_display_matches_its_flag_spelling() {
    assert_eq!(ResourceScope::Mutable.to_string(), "mutable");
    assert_eq!(ResourceScope::Immutable.to_string(), "immutable");
    assert_eq!(ResourceScope::All.to_string(), "all");
}

#[test]
fn resource_ref_display_is_kind_slash_name() {
    assert_eq!(
        resource_ref("Deployment", "web", false).to_string(),
        "Deployment/web"
    );
}

#[test]
fn resource_ref_in_scope_all_always_matches() {
    assert!(resource_ref("Deployment", "web", false).in_scope(ResourceScope::All));
    assert!(resource_ref("Secret", "creds", true).in_scope(ResourceScope::All));
}

#[test]
fn resource_ref_in_scope_mutable_excludes_immutable_resources() {
    assert!(resource_ref("Deployment", "web", false).in_scope(ResourceScope::Mutable));
    assert!(!resource_ref("Secret", "creds", true).in_scope(ResourceScope::Mutable));
}

#[test]
fn resource_ref_in_scope_immutable_excludes_mutable_resources() {
    assert!(!resource_ref("Deployment", "web", false).in_scope(ResourceScope::Immutable));
    assert!(resource_ref("Secret", "creds", true).in_scope(ResourceScope::Immutable));
}
