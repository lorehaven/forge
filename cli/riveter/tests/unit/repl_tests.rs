use crate::fake_cluster::{FakeCluster, with_cluster, with_clusters};
use riveter::render::{RenderedManifest, ResourceRef, ResourceScope};
use riveter::repl::{
    LiveResource, WaitPolicy, connect, describe, find_orphans, note_skipped, parse_args,
    parse_scope_value, print_resource_list, report_prune, unknown_option,
};

/// `parse_args` skips element 0, the way the REPL hands it the command word.
fn apply(line: &str) -> quench_core::Result<riveter::repl::ParsedArgs> {
    let args = line.split_whitespace().collect::<Vec<_>>();
    parse_args(&args, ResourceScope::Mutable, true)
}

fn list(line: &str) -> quench_core::Result<riveter::repl::ParsedArgs> {
    let args = line.split_whitespace().collect::<Vec<_>>();
    parse_args(&args, ResourceScope::All, false)
}

#[quench_test::test]
fn render_defaults_to_the_same_scope_as_apply() {
    // A render that previewed a wider scope than apply acts on would make the
    // obvious "render, check, apply" workflow lie.
    let render = parse_args(&["render"], ResourceScope::Mutable, false).expect("should parse");
    let apply = apply("apply").expect("should parse");

    assert_eq!(render.scope, apply.scope);
    assert_eq!(render.scope, ResourceScope::Mutable);
}

#[quench_test::test]
fn bare_command_takes_the_default_scope() {
    let parsed = apply("apply").expect("bare apply should parse");
    assert!(!parsed.dry_run);
    assert!(parsed.selector.is_empty());
    assert_eq!(parsed.scope, ResourceScope::Mutable);
}

#[quench_test::test]
fn scope_accepts_both_the_space_and_equals_forms() {
    // clap takes either on the CLI, so the REPL must agree — otherwise
    // `--scope=immutable` silently falls back to the default.
    for line in ["list --scope immutable", "list --scope=immutable"] {
        let parsed = list(line).expect("scope should parse");
        assert_eq!(parsed.scope, ResourceScope::Immutable, "`{line}`");
    }
}

#[quench_test::test]
fn scope_value_is_case_insensitive() {
    assert_eq!(
        list("list --scope ALL").expect("scope should parse").scope,
        ResourceScope::All
    );
}

#[quench_test::test]
fn unknown_option_is_rejected_rather_than_dropped() {
    // The whole point: a typo'd safety flag must not turn into a live apply.
    let err = apply("apply --dry-runn").expect_err("typo should be rejected");
    let msg = err.to_string();
    assert!(msg.contains("--dry-runn"), "{msg}");
    assert!(msg.contains("accepted options"), "{msg}");
}

#[quench_test::test]
fn dry_run_is_only_accepted_where_it_does_something() {
    assert!(
        apply("apply --dry-run")
            .expect("apply takes --dry-run")
            .dry_run
    );

    let err = list("list --dry-run").expect_err("list does not take --dry-run");
    assert!(err.to_string().contains("only `apply`"), "{err}");
}

#[quench_test::test]
fn a_near_miss_on_scope_names_the_real_flag() {
    let err = list("list --scoped immutable").expect_err("should be rejected");
    assert!(err.to_string().contains("did you mean `--scope`"), "{err}");
}

#[quench_test::test]
fn scope_requires_a_value() {
    for line in ["list --scope", "list --scope="] {
        let err = list(line).expect_err(&format!("`{line}` should be rejected"));
        assert!(
            err.to_string().contains("missing value for --scope"),
            "{line}"
        );
    }
}

#[quench_test::test]
fn invalid_scope_value_is_rejected() {
    let err = list("list --scope sideways").expect_err("should be rejected");
    assert!(err.to_string().contains("sideways"), "{err}");
}

#[quench_test::test]
fn targets_parse_alongside_flags_in_any_order() {
    let parsed = apply("apply --scope=all deployment/api --dry-run service/api")
        .expect("mixed line should parse");

    assert!(parsed.dry_run);
    assert_eq!(parsed.scope, ResourceScope::All);
    assert!(parsed.selector.matches("deployment", "api"));
    assert!(parsed.selector.matches("service", "api"));
    assert!(!parsed.selector.matches("configmap", "api"));
}

#[quench_test::test]
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
        phases: Vec::new(),
    }
}

fn resource(kind: &str, name: &str) -> ResourceRef {
    ResourceRef {
        kind: kind.to_string(),
        name: name.to_string(),
        immutable: false,
    }
}

#[quench_test::test]
fn describe_lists_every_selected_resource_as_kind_slash_name() {
    let rendered = manifest(
        vec![resource("Deployment", "web"), resource("Service", "web")],
        vec![],
    );
    assert_eq!(describe(&rendered), "Deployment/web, Service/web");
}

#[quench_test::test]
fn describe_is_empty_when_nothing_is_selected() {
    assert_eq!(describe(&manifest(vec![], vec![])), "");
}

#[quench_test::test]
fn note_skipped_is_none_when_nothing_was_left_out() {
    assert_eq!(note_skipped(&manifest(vec![], vec![])), None);
}

#[quench_test::test]
fn note_skipped_names_the_left_out_resources() {
    let rendered = manifest(vec![], vec![resource("ConfigMap", "immutable-cfg")]);
    let note = note_skipped(&rendered).unwrap();
    assert!(note.contains("1 resource(s)"));
    assert!(note.contains("ConfigMap/immutable-cfg"));
    assert!(note.contains("--scope all"));
}

#[quench_test::test]
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

#[quench_test::test]
fn live_resource_display_is_kind_slash_name() {
    let resource = LiveResource {
        kind: "deployment".to_string(),
        name: "my-app".to_string(),
    };
    assert_eq!(resource.to_string(), "deployment/my-app");
}

#[quench_test::test]
fn wait_policy_default_waits_up_to_five_minutes() {
    let policy = WaitPolicy::default();
    assert!(policy.enabled);
    assert_eq!(policy.timeout_seconds, 300);
}

/// A cluster holding objects riveter labelled for `test-env`, as `kind name [owner]`, in the namespace a connection defaults to.
fn labelled_cluster(objects: &[(&str, &str, &str, &str)]) -> FakeCluster {
    let cluster = FakeCluster::start();
    for (plural, kind, name, extra) in objects {
        cluster.insert(
            plural,
            &format!(
                r#"{{"apiVersion":"v1","kind":"{kind}","metadata":{{"name":"{name}","namespace":"default","labels":{{"app.kubernetes.io/managed-by":"riveter","env":"test-env"}}{extra}}}}}"#
            ),
        );
    }
    cluster
}

fn orphans_of(cluster: &FakeCluster, rendered: &RenderedManifest) -> Vec<LiveResource> {
    with_cluster(cluster, || {
        let connection = connect(rendered).unwrap();
        find_orphans(&connection, "test-env", rendered).unwrap()
    })
}

#[quench_test::test]
fn find_orphans_reports_live_resources_the_overlay_no_longer_selects() {
    let cluster = labelled_cluster(&[
        ("deployments", "Deployment", "my-app", ""),
        ("services", "Service", "my-app", ""),
    ]);
    let rendered = manifest(vec![resource("Deployment", "my-app")], vec![]);
    let orphans = orphans_of(&cluster, &rendered);

    assert_eq!(orphans.len(), 1);
    assert_eq!(orphans[0].kind, "service");
    assert_eq!(orphans[0].name, "my-app");
}

#[quench_test::test]
fn find_orphans_ignores_what_carries_other_labels() {
    let cluster = labelled_cluster(&[("deployments", "Deployment", "mine", "")]);
    cluster.insert(
        "deployments",
        r#"{"apiVersion":"apps/v1","kind":"Deployment","metadata":{"name":"someone-elses","namespace":"default","labels":{"app.kubernetes.io/managed-by":"helm","env":"test-env"}}}"#,
    );
    let orphans = orphans_of(&cluster, &manifest(vec![], vec![]));
    assert_eq!(orphans.len(), 1, "{orphans:?}");
    assert_eq!(orphans[0].to_string(), "deployment/mine");
}

#[quench_test::test]
fn find_orphans_leaves_alone_what_the_cluster_derived_from_declared_resources() {
    // The listing that used to propose deleting every certificate, endpoint and endpoint slice: all of
    // them carry riveter's labels, none of them is riveter's.
    let owner = |kind: &str| format!(r#","ownerReferences":[{{"kind":"{kind}","name":"x"}}]"#);
    let cluster = labelled_cluster(&[
        ("deployments", "Deployment", "my-app", ""),
        ("endpoints", "Endpoints", "my-app", ""),
        ("jobs", "Job", "stale-release", ""),
        (
            "pods",
            "Pod",
            "my-app-5f9b6b444b-abcde",
            &owner("ReplicaSet"),
        ),
    ]);
    cluster.insert(
        "services",
        r#"{"apiVersion":"v1","kind":"Service","metadata":{"name":"my-app","namespace":"default","labels":{"app.kubernetes.io/managed-by":"riveter","env":"test-env"}},"spec":{"selector":{"app":"my-app"}}}"#,
    );
    let rendered = manifest(
        vec![
            resource("Deployment", "my-app"),
            resource("Service", "my-app"),
        ],
        vec![],
    );
    let orphans = orphans_of(&cluster, &rendered);

    // Only the Job the overlay genuinely stopped declaring... and the Deployment, which the list cannot tell is declared.
    let names: Vec<String> = orphans.iter().map(ToString::to_string).collect();
    assert!(
        names.contains(&"job/stale-release".to_string()),
        "{names:?}"
    );
    assert!(
        !names
            .iter()
            .any(|n| n.starts_with("pod/") || n.starts_with("endpoints/")),
        "{names:?}"
    );
}

#[quench_test::test]
fn find_orphans_is_empty_when_the_cluster_has_nothing() {
    let cluster = FakeCluster::start();
    let orphans = orphans_of(
        &cluster,
        &manifest(vec![resource("Deployment", "my-app")], vec![]),
    );
    assert_eq!(orphans, [] as [riveter::repl::LiveResource; 0]);
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
        phases: Vec::new(),
    }
}

#[quench_test::test]
fn the_connection_is_bound_to_the_overlays_context_when_it_pins_one() {
    let current = FakeCluster::start();
    let pinned = FakeCluster::start();
    pinned.insert(
        "namespaces",
        r#"{"apiVersion":"v1","kind":"Namespace","metadata":{"name":"only-on-prod"}}"#,
    );

    with_clusters(&[("dev", &current), ("prod-cluster", &pinned)], || {
        let bound = connect(&manifest_with_context(Some("prod-cluster"))).unwrap();
        assert_eq!(bound.namespace_exists("only-on-prod"), Some(true));

        // No pin: the kubeconfig's current context, which is the other cluster.
        let ambient = connect(&manifest_with_context(None)).unwrap();
        assert_eq!(ambient.namespace_exists("only-on-prod"), Some(false));
    });
}

#[quench_test::test]
fn an_unknown_context_is_refused() {
    let cluster = FakeCluster::start();
    with_cluster(&cluster, || {
        let err = connect(&manifest_with_context(Some("nope"))).unwrap_err();
        assert!(err.to_string().contains("nope"), "{err}");
    });
}

#[quench_test::test]
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

#[quench_test::test]
fn parse_scope_value_rejects_anything_else() {
    let error = parse_scope_value("bogus").unwrap_err();
    assert!(error.to_string().contains("invalid --scope value"));
}

#[quench_test::test]
fn unknown_option_hints_at_apply_only_for_dry_run() {
    assert!(unknown_option("--dry-run", false).contains("only `apply` takes --dry-run"));
    assert!(!unknown_option("--foo", false).contains("only `apply`"));
}

#[quench_test::test]
fn unknown_option_lists_dry_run_only_when_it_is_accepted() {
    assert!(unknown_option("--foo", true).contains("--dry-run"));
    assert!(!unknown_option("--foo", false).contains("--dry-run"));
}
