use anvil::commands::lint::{format_args, lint_args};

#[test]
fn lint_args_bare_call_has_the_separator_but_no_deny() {
    assert_eq!(lint_args(false, false, false), vec!["clippy", "--"]);
}

#[test]
fn lint_args_all_targets_and_all_features() {
    assert_eq!(
        lint_args(true, true, false),
        vec!["clippy", "--all-targets", "--all-features", "--"]
    );
}

#[test]
fn lint_args_deny_warnings_appends_after_the_separator() {
    assert_eq!(
        lint_args(false, false, true),
        vec!["clippy", "--", "-D", "warnings"]
    );
}

#[test]
fn format_args_defaults_to_no_check() {
    assert_eq!(format_args(false), vec!["+nightly", "fmt"]);
}

#[test]
fn format_args_check_mode() {
    assert_eq!(format_args(true), vec!["+nightly", "fmt", "--check"]);
}
