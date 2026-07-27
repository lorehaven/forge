use riveter::render::ResourceScope;
use riveter::repl::parse_args;

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
        let err = list(line).expect_err("`{line}` should be rejected");
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
