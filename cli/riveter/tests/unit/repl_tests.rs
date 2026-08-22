use crate::env_support::cwd_lock;
use riveter::render::{RenderedManifest, ResourceRef, ResourceScope};
use riveter::repl::{
    LiveResource, WaitPolicy, describe, find_orphans, kubectl, note_skipped, parse_args,
    parse_scope_value, print_resource_list, report_prune, unknown_option,
};
use std::os::unix::fs::PermissionsExt;

/// `parse_args` skips element 0, the way the REPL hands it the command word.
fn apply(line: &str) -> anyhow::Result<riveter::repl::ParsedArgs> {
    let args = line.split_whitespace().collect::<Vec<_>>();
    parse_args(&args, ResourceScope::Mutable, true)
}

fn list(line: &str) -> anyhow::Result<riveter::repl::ParsedArgs> {
    let args = line.split_whitespace().collect::<Vec<_>>();
    parse_args(&args, ResourceScope::All, false)
}

#[test]
fn render_defaults_to_the_same_scope_as_apply() {
    // A render that previewed a wider scope than apply acts on would make the
    // obvious "render, check, apply" workflow lie.
    let render = parse_args(&["render"], ResourceScope::Mutable, false).expect("should parse");
    let apply = apply("apply").expect("should parse");

    assert_eq!(render.scope, apply.scope);
    assert_eq!(render.scope, ResourceScope::Mutable);
}

#[test]
fn bare_command_takes_the_default_scope() {
    let parsed = apply("apply").expect("bare apply should parse");
    assert!(!parsed.dry_run);
    assert!(parsed.selector.is_empty());
    assert_eq!(parsed.scope, ResourceScope::Mutable);
}

#[test]
fn scope_accepts_both_the_space_and_equals_forms() {
    // clap takes either on the CLI, so the REPL must agree — otherwise
    // `--scope=immutable` silently falls back to the default.
    for line in ["list --scope immutable", "list --scope=immutable"] {
        let parsed = list(line).expect("scope should parse");
        assert_eq!(parsed.scope, ResourceScope::Immutable, "`{line}`");
    }
}

#[test]
fn scope_value_is_case_insensitive() {
    assert_eq!(
        list("list --scope ALL").expect("scope should parse").scope,
        ResourceScope::All
    );
}

#[test]
fn unknown_option_is_rejected_rather_than_dropped() {
    // The whole point: a typo'd safety flag must not turn into a live apply.
    let err = apply("apply --dry-runn").expect_err("typo should be rejected");
    let msg = err.to_string();
    assert!(msg.contains("--dry-runn"), "{msg}");
    assert!(msg.contains("accepted options"), "{msg}");
}

#[test]
fn dry_run_is_only_accepted_where_it_does_something() {
    assert!(
        apply("apply --dry-run")
            .expect("apply takes --dry-run")
            .dry_run
    );

    let err = list("list --dry-run").expect_err("list does not take --dry-run");
    assert!(err.to_string().contains("only `apply`"), "{err}");
}

#[test]
fn a_near_miss_on_scope_names_the_real_flag() {
    let err = list("list --scoped immutable").expect_err("should be rejected");
    assert!(err.to_string().contains("did you mean `--scope`"), "{err}");
}

#[test]
fn scope_requires_a_value() {
    for line in ["list --scope", "list --scope="] {
        let err = list(line).expect_err(&format!("`{line}` should be rejected"));
        assert!(
            err.to_string().contains("missing value for --scope"),
            "{line}"
        );
    }
}

#[test]
fn invalid_scope_value_is_rejected() {
    let err = list("list --scope sideways").expect_err("should be rejected");
    assert!(err.to_string().contains("sideways"), "{err}");
}

#[test]
fn targets_parse_alongside_flags_in_any_order() {
    let parsed = apply("apply --scope=all deployment/api --dry-run service/api")
        .expect("mixed line should parse");

    assert!(parsed.dry_run);
    assert_eq!(parsed.scope, ResourceScope::All);
    assert!(parsed.selector.matches("deployment", "api"));
    assert!(parsed.selector.matches("service", "api"));
    assert!(!parsed.selector.matches("configmap", "api"));
}

#[test]
fn the_scope_value_is_not_mistaken_for_a_target() {
    let parsed = list("list --scope mutable").expect("should parse");
    assert!(
        parsed.selector.is_empty(),
        "`mutable` was taken as a target"
    );
}

fn manifest(selected: Vec<ResourceRef>, skipped: Vec<ResourceRef>) -> RenderedManifest {
    RenderedManifest {
        path: "/tmp/manifest.yaml".to_string(),
        resource_count: selected.len(),
        selected,
        kube_context: None,
        skipped_out_of_scope: skipped,
        namespace: Some("default".to_string()),
        creates_namespace: false,
    }
}

fn resource(kind: &str, name: &str) -> ResourceRef {
    ResourceRef {
        kind: kind.to_string(),
        name: name.to_string(),
        immutable: false,
    }
}

#[test]
fn describe_lists_every_selected_resource_as_kind_slash_name() {
    let rendered = manifest(
        vec![resource("Deployment", "web"), resource("Service", "web")],
        vec![],
    );
    assert_eq!(describe(&rendered), "Deployment/web, Service/web");
}

#[test]
fn describe_is_empty_when_nothing_is_selected() {
    assert_eq!(describe(&manifest(vec![], vec![])), "");
}

#[test]
fn note_skipped_is_none_when_nothing_was_left_out() {
    assert_eq!(note_skipped(&manifest(vec![], vec![])), None);
}

#[test]
fn note_skipped_names_the_left_out_resources() {
    let rendered = manifest(vec![], vec![resource("ConfigMap", "immutable-cfg")]);
    let note = note_skipped(&rendered).unwrap();
    assert!(note.contains("1 resource(s)"));
    assert!(note.contains("ConfigMap/immutable-cfg"));
    assert!(note.contains("--scope all"));
}

#[test]
fn print_resource_list_and_report_prune_do_not_panic_on_empty_or_populated_input() {
    print_resource_list(&[]);
    print_resource_list(&[resource("Deployment", "web")]);

    report_prune(&[], false);
    let orphan = LiveResource {
        kind: "deployment".to_string(),
        name: "old-app".to_string(),
    };
    report_prune(std::slice::from_ref(&orphan), true);
    report_prune(&[orphan], false);
}

#[test]
fn live_resource_display_is_kind_slash_name() {
    let resource = LiveResource {
        kind: "deployment".to_string(),
        name: "my-app".to_string(),
    };
    assert_eq!(resource.to_string(), "deployment/my-app");
}

#[test]
fn wait_policy_default_waits_up_to_five_minutes() {
    let policy = WaitPolicy::default();
    assert!(policy.enabled);
    assert_eq!(policy.timeout_seconds, 300);
}

/// Writes an executable `kubectl` shim into a fresh temp dir printing
/// `output` to stdout and exiting 0, prepends that dir to `$PATH`, and runs
/// `body` with it in effect. Holds `cwd_lock` for the duration - `$PATH` is
/// process-global, exactly like the cwd that lock already protects, and
/// nothing else in this binary needs a fake `kubectl` on the path at the
/// same time.
fn with_fake_kubectl<T>(output: &str, body: impl FnOnce() -> T) -> T {
    let _guard = cwd_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let dir = tempfile::tempdir().unwrap();
    let script_path = dir.path().join("kubectl");
    std::fs::write(
        &script_path,
        format!("#!/bin/sh\ncat <<'EOF_OUTPUT'\n{output}\nEOF_OUTPUT\n"),
    )
    .unwrap();
    std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755)).unwrap();

    let original_path = std::env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{original_path}", dir.path().display());
    envmnt::set("PATH", &new_path);

    let result = body();

    envmnt::set("PATH", &original_path);
    result
}

#[test]
fn find_orphans_reports_live_resources_the_overlay_no_longer_selects() {
    with_fake_kubectl("deployment.apps/my-app\nservice/my-app", || {
        let rendered = manifest(vec![resource("Deployment", "my-app")], vec![]);
        let orphans = find_orphans("test-env", &rendered).unwrap();

        assert_eq!(orphans.len(), 1);
        assert_eq!(orphans[0].kind, "service");
        assert_eq!(orphans[0].name, "my-app");
    });
}

#[test]
fn find_orphans_is_empty_when_the_cluster_has_nothing_extra() {
    with_fake_kubectl("deployment.apps/my-app", || {
        let rendered = manifest(vec![resource("Deployment", "my-app")], vec![]);
        let orphans = find_orphans("test-env", &rendered).unwrap();
        assert!(orphans.is_empty());
    });
}

fn manifest_with_context(kube_context: Option<&str>) -> RenderedManifest {
    RenderedManifest {
        path: "/tmp/manifest.yaml".to_string(),
        resource_count: 0,
        selected: Vec::new(),
        kube_context: kube_context.map(str::to_string),
        skipped_out_of_scope: Vec::new(),
        namespace: None,
        creates_namespace: false,
    }
}

#[test]
fn kubectl_binds_the_overlays_context_when_it_pins_one() {
    let cmd = kubectl(&manifest_with_context(Some("prod-cluster")));
    assert_eq!(cmd.get_program(), "kubectl");
    let args: Vec<&str> = cmd.get_args().map(|a| a.to_str().unwrap()).collect();
    assert_eq!(args, vec!["--context", "prod-cluster"]);
}

#[test]
fn kubectl_adds_no_context_flag_when_the_overlay_pins_none() {
    let cmd = kubectl(&manifest_with_context(None));
    assert_eq!(cmd.get_args().count(), 0);
}

#[test]
fn parse_scope_value_accepts_every_case_insensitive_spelling() {
    assert!(matches!(
        parse_scope_value("Mutable").unwrap(),
        ResourceScope::Mutable
    ));
    assert!(matches!(
        parse_scope_value("IMMUTABLE").unwrap(),
        ResourceScope::Immutable
    ));
    assert!(matches!(
        parse_scope_value("all").unwrap(),
        ResourceScope::All
    ));
}

#[test]
fn parse_scope_value_rejects_anything_else() {
    let error = parse_scope_value("bogus").unwrap_err();
    assert!(error.to_string().contains("invalid --scope value"));
}

#[test]
fn unknown_option_hints_at_apply_only_for_dry_run() {
    assert!(unknown_option("--dry-run", false).contains("only `apply` takes --dry-run"));
    assert!(!unknown_option("--foo", false).contains("only `apply`"));
}

#[test]
fn unknown_option_lists_dry_run_only_when_it_is_accepted() {
    assert!(unknown_option("--foo", true).contains("--dry-run"));
    assert!(!unknown_option("--foo", false).contains("--dry-run"));
}
