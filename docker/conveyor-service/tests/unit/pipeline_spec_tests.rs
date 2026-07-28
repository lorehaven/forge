//! Unit tests for `pipeline/spec.rs`: ref globbing and trigger matching.

use conveyor_service::pipeline::spec::{Step, Triggers, glob_match};

// ---------------------------------------------------------------------------
// Globs
// ---------------------------------------------------------------------------

#[test]
fn a_literal_pattern_matches_only_itself() {
    assert!(glob_match("master", "master"));
    assert!(!glob_match("master", "master-2"));
    assert!(!glob_match("master", "maste"));
}

#[test]
fn a_bare_star_matches_everything_including_nothing() {
    assert!(glob_match("*", "master"));
    assert!(glob_match("*", "release/1.2"));
    assert!(glob_match("*", ""));
}

#[test]
fn a_star_crosses_slashes() {
    // `release/*` matching `release/1.2` is the whole point; a pattern language
    // where it does not is one people get wrong every time.
    assert!(glob_match("release/*", "release/1.2"));
    assert!(glob_match("release/*", "release/1.2/hotfix"));
    assert!(!glob_match("release/*", "releases/1.2"));
}

#[test]
fn a_trailing_star_matches_the_empty_remainder() {
    assert!(glob_match("release/*", "release/"));
    assert!(glob_match("main*", "main"));
}

#[test]
fn a_leading_star_anchors_the_end() {
    assert!(glob_match("*-hotfix", "1.2-hotfix"));
    assert!(!glob_match("*-hotfix", "1.2-hotfixes"));
}

#[test]
fn several_stars_backtrack_correctly() {
    assert!(glob_match("*/*/*", "a/b/c"));
    assert!(glob_match("a*c*e", "abcde"));
    assert!(!glob_match("a*c*e", "abcd"));
    // The case naive matching gets wrong: the first `*` must give characters
    // back once the literal after it fails to line up.
    assert!(glob_match("*abc", "zzabcabc"));
}

#[test]
fn consecutive_stars_are_harmless() {
    assert!(glob_match("**", "anything"));
    assert!(glob_match("a**b", "ab"));
}

#[test]
fn an_empty_pattern_matches_only_the_empty_ref() {
    assert!(glob_match("", ""));
    assert!(!glob_match("", "master"));
}

// ---------------------------------------------------------------------------
// Triggers
// ---------------------------------------------------------------------------

fn triggers(push: &[&str], pull_request: &[&str], tag: &[&str]) -> Triggers {
    let own = |values: &[&str]| values.iter().map(|s| (*s).to_string()).collect();
    Triggers {
        push: own(push),
        pull_request: own(pull_request),
        tag: own(tag),
    }
}

#[test]
fn a_push_matches_its_branch_patterns() {
    let on = triggers(&["master", "release/*"], &[], &[]);
    assert!(on.allows("push", "refs/heads/master"));
    assert!(on.allows("push", "refs/heads/release/1.2"));
    assert!(!on.allows("push", "refs/heads/topic"));
}

#[test]
fn a_bare_ref_is_matched_as_a_branch() {
    let on = triggers(&["master"], &[], &[]);
    assert!(on.allows("push", "master"));
}

#[test]
fn a_pull_request_uses_its_own_patterns() {
    let on = triggers(&["master"], &["*"], &[]);
    assert!(on.allows("pull_request", "refs/heads/topic"));
    assert!(!on.allows("push", "refs/heads/topic"));
}

#[test]
fn a_tag_push_does_not_fall_back_to_the_branch_patterns() {
    // Otherwise `push = ["*"]` fires on every release tag as well as every
    // branch, and a tag build runs the pipeline twice over.
    let on = triggers(&["*"], &[], &[]);
    assert!(!on.allows("push", "refs/tags/v1.0.0"));
}

#[test]
fn a_tag_push_matches_its_tag_patterns() {
    let on = triggers(&[], &[], &["v*"]);
    assert!(on.allows("push", "refs/tags/v1.0.0"));
    assert!(!on.allows("push", "refs/tags/nightly"));
}

#[test]
fn a_pull_request_event_on_a_tag_ref_never_matches() {
    let on = triggers(&["*"], &["*"], &["*"]);
    assert!(!on.allows("pull_request", "refs/tags/v1.0.0"));
}

#[test]
fn an_empty_pattern_list_means_the_event_never_triggers() {
    let on = triggers(&[], &[], &[]);
    assert!(!on.allows("push", "refs/heads/master"));
    assert!(!on.allows("pull_request", "refs/heads/master"));
}

#[test]
fn a_manual_trigger_is_always_allowed() {
    // Somebody asked for this run by name. Refusing would leave them no way to
    // run a pipeline whose patterns do not cover the branch they are on.
    let on = triggers(&[], &[], &[]);
    assert!(on.allows("manual", "refs/heads/anything"));
    assert!(on.allows("manual", "refs/tags/v1.0.0"));
}

#[test]
fn an_unrecognised_event_never_triggers() {
    let on = triggers(&["*"], &["*"], &["*"]);
    assert!(!on.allows("issue_comment", "refs/heads/master"));
}

#[test]
fn the_default_builds_every_push_and_nothing_else() {
    let on = Triggers::default();
    assert!(on.allows("push", "refs/heads/anything"));
    assert!(!on.allows("pull_request", "refs/heads/anything"));
    assert!(!on.allows("push", "refs/tags/v1.0.0"));
}

// ---------------------------------------------------------------------------
// Steps
// ---------------------------------------------------------------------------

#[test]
fn every_declared_kind_round_trips() {
    for kind in Step::KINDS {
        let step = Step::new(kind, "do a thing").unwrap_or_else(|| panic!("{kind} should build"));
        assert_eq!(step.kind(), kind);
        assert_eq!(step.command(), "do a thing");
    }
}

#[test]
fn an_unknown_kind_builds_nothing() {
    assert!(Step::new("npm", "install").is_none());
}
