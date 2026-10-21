use anvil::commands::lint::{
    format_args, format_summary, lint_args, package_name, tally_diagnostic,
};
use std::collections::{BTreeMap, HashSet};

#[test]
fn lint_args_bare_call_has_the_separator_but_no_deny() {
    assert_eq!(lint_args(false, false, false, false), vec!["clippy", "--"]);
}

#[test]
fn lint_args_all_targets_and_all_features() {
    assert_eq!(
        lint_args(true, true, false, false),
        vec!["clippy", "--all-targets", "--all-features", "--"]
    );
}

#[test]
fn lint_args_deny_warnings_appends_after_the_separator() {
    assert_eq!(
        lint_args(false, false, true, false),
        vec!["clippy", "--", "-D", "warnings"]
    );
}

#[test]
fn lint_args_json_adds_clippys_message_format_before_other_flags() {
    assert_eq!(
        lint_args(true, false, false, true),
        vec!["clippy", "--message-format=json", "--all-targets", "--"]
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

#[test]
fn package_name_handles_both_id_forms() {
    assert_eq!(
        package_name("path+file:///w/cli/anvil#anvil@0.4.2"),
        "anvil"
    );
    assert_eq!(package_name("path+file:///w/cli/anvil#0.4.2"), "anvil");
    assert_eq!(
        package_name("anvil 0.4.2 (path+file:///w/cli/anvil)"),
        "anvil"
    );
}

#[test]
fn tally_counts_real_diagnostics_once_and_skips_summaries() {
    let diag = r#"{"reason":"compiler-message","package_id":"path+file:///w/a#a@1.0.0","message":{"level":"warning","code":{"code":"x"},"spans":[{}],"rendered":"warn\n"}}"#;
    let summary = r#"{"reason":"compiler-message","package_id":"path+file:///w/a#a@1.0.0","message":{"level":"error","code":null,"spans":[],"rendered":"aborting\n"}}"#;
    let mut counts = BTreeMap::new();
    let mut seen = HashSet::new();
    assert!(tally_diagnostic(diag, &mut counts, &mut seen).is_some());
    assert!(tally_diagnostic(diag, &mut counts, &mut seen).is_some());
    assert!(tally_diagnostic(summary, &mut counts, &mut seen).is_none());
    assert_eq!(counts["a"], 1);
}

#[test]
fn summary_sorts_by_count_and_totals() {
    let counts = BTreeMap::from([("a".to_string(), 1), ("bb".to_string(), 3)]);
    assert_eq!(
        format_summary(&counts),
        "module  issues\nbb      3\na       1\ntotal   4\n"
    );
}
