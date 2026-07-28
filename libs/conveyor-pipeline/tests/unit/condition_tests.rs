//! Unit tests for `pipeline/condition.rs`.

use conveyor_pipeline::condition::{Condition, ConditionError, EvalContext, Variable};

fn on_branch(branch: &str) -> EvalContext {
    EvalContext::new("push", &format!("refs/heads/{branch}"), "abc1234")
}

fn on_tag(tag: &str) -> EvalContext {
    EvalContext::new("push", &format!("refs/tags/{tag}"), "abc1234")
}

fn holds(source: &str, context: &EvalContext) -> bool {
    Condition::parse(source)
        .unwrap_or_else(|e| panic!("{source:?} should parse: {e}"))
        .evaluate(context)
}

// ---------------------------------------------------------------------------
// Context
// ---------------------------------------------------------------------------

#[test]
fn a_branch_ref_fills_branch_and_leaves_tag_empty() {
    let context = on_branch("master");
    assert_eq!(context.branch, "master");
    assert_eq!(context.tag, "");
}

#[test]
fn a_tag_ref_fills_tag_and_leaves_branch_empty() {
    // This is what makes `tag != ''` mean "only on a tag". If a tag build also
    // reported a branch, every `branch == 'master'` deploy would fire on tags.
    let context = on_tag("v1.0.0");
    assert_eq!(context.tag, "v1.0.0");
    assert_eq!(context.branch, "");
}

#[test]
fn a_bare_ref_is_read_as_a_branch() {
    let context = EvalContext::new("manual", "master", "abc");
    assert_eq!(context.branch, "master");
    assert_eq!(context.tag, "");
}

#[test]
fn a_nested_branch_keeps_its_slashes() {
    assert_eq!(on_branch("release/1.2").branch, "release/1.2");
}

// ---------------------------------------------------------------------------
// Evaluation
// ---------------------------------------------------------------------------

#[test]
fn equality_compares_the_named_variable() {
    let context = on_branch("master");
    assert!(holds("branch == 'master'", &context));
    assert!(!holds("branch == 'develop'", &context));
}

#[test]
fn inequality_is_the_negation() {
    let context = on_branch("master");
    assert!(holds("branch != 'develop'", &context));
    assert!(!holds("branch != 'master'", &context));
}

#[test]
fn every_variable_is_readable() {
    let context = EvalContext::new("pull_request", "refs/heads/topic", "deadbeef");
    assert!(holds("branch == 'topic'", &context));
    assert!(holds("event == 'pull_request'", &context));
    assert!(holds("sha == 'deadbeef'", &context));
    assert!(holds("tag == ''", &context));
}

#[test]
fn only_on_a_tag_is_expressible() {
    assert!(holds("tag != ''", &on_tag("v1.0.0")));
    assert!(!holds("tag != ''", &on_branch("master")));
}

#[test]
fn conjunction_requires_both_sides() {
    let context = on_branch("master");
    assert!(holds("branch == 'master' && event == 'push'", &context));
    assert!(!holds("branch == 'master' && event == 'manual'", &context));
}

#[test]
fn disjunction_requires_either_side() {
    let context = on_branch("develop");
    assert!(holds("branch == 'master' || branch == 'develop'", &context));
    assert!(!holds("branch == 'main' || branch == 'trunk'", &context));
}

#[test]
fn conjunction_binds_tighter_than_disjunction() {
    // `a || b && c` is `a || (b && c)`. Read the other way, this condition
    // would be false, and a deploy stage would silently stop firing on master.
    let context = on_branch("master");
    assert!(holds(
        "branch == 'master' || branch == 'develop' && event == 'manual'",
        &context
    ));

    let context = on_branch("develop");
    assert!(!holds(
        "branch == 'main' || branch == 'develop' && event == 'manual'",
        &context
    ));
}

#[test]
fn both_quote_styles_are_accepted() {
    let context = on_branch("master");
    assert!(holds("branch == \"master\"", &context));
    assert!(holds("branch == 'master'", &context));
}

#[test]
fn whitespace_is_not_significant() {
    let context = on_branch("master");
    assert!(holds("   branch=='master'   ", &context));
    assert!(holds("branch\t==\t'master'", &context));
}

// ---------------------------------------------------------------------------
// Round-tripping
// ---------------------------------------------------------------------------

#[test]
fn a_condition_renders_back_to_something_that_parses() {
    let source = "branch == 'master' && event != 'manual' || tag != ''";
    let parsed = Condition::parse(source).expect("parses");
    let rendered = parsed.to_string();
    assert_eq!(
        Condition::parse(&rendered).expect("re-parses"),
        parsed,
        "rendered as {rendered:?}"
    );
}

#[test]
fn variables_parse_back_from_what_they_render() {
    for variable in Variable::ALL {
        assert_eq!(Variable::parse(variable.as_str()), Some(variable));
    }
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[test]
fn an_empty_condition_is_an_error() {
    assert_eq!(Condition::parse(""), Err(ConditionError::Empty));
    assert_eq!(Condition::parse("   "), Err(ConditionError::Empty));
}

#[test]
fn an_unknown_variable_is_named_along_with_the_known_ones() {
    let error = Condition::parse("author == 'me'").expect_err("should not parse");
    let message = error.to_string();
    assert!(message.contains("author"), "{message}");
    assert!(message.contains("branch"), "{message}");
    assert!(message.contains("event"), "{message}");
}

#[test]
fn a_single_equals_is_explained() {
    // The mistake everyone makes once. Saying "unexpected character '='" and
    // stopping there would be technically true and useless.
    let message = Condition::parse("branch = 'master'")
        .expect_err("should not parse")
        .to_string();
    assert!(message.contains("'=='"), "{message}");
}

#[test]
fn single_character_connectives_are_explained() {
    let message = Condition::parse("branch == 'a' & event == 'push'")
        .expect_err("should not parse")
        .to_string();
    assert!(message.contains("'&&'"), "{message}");

    let message = Condition::parse("branch == 'a' | event == 'push'")
        .expect_err("should not parse")
        .to_string();
    assert!(message.contains("'||'"), "{message}");
}

#[test]
fn an_unquoted_value_is_rejected() {
    // `branch == master` reads as comparing two variables, which the language
    // does not have; accepting it would mean guessing which one was meant.
    let error = Condition::parse("branch == master").expect_err("should not parse");
    assert!(
        matches!(error, ConditionError::ExpectedValue { .. }),
        "{error:?}"
    );
}

#[test]
fn a_missing_comparison_is_rejected() {
    let error = Condition::parse("branch 'master'").expect_err("should not parse");
    assert!(
        matches!(error, ConditionError::ExpectedComparison { .. }),
        "{error:?}"
    );
}

#[test]
fn an_unterminated_string_is_rejected() {
    let error = Condition::parse("branch == 'master").expect_err("should not parse");
    assert_eq!(error, ConditionError::UnterminatedString { quote: '\'' });
}

#[test]
fn a_dangling_connective_is_rejected() {
    assert!(Condition::parse("branch == 'a' &&").is_err());
    assert!(Condition::parse("|| branch == 'a'").is_err());
}

#[test]
fn trailing_junk_is_rejected() {
    let error = Condition::parse("branch == 'a' 'b'").expect_err("should not parse");
    assert!(
        matches!(error, ConditionError::Trailing { .. }),
        "{error:?}"
    );
}

#[test]
fn parentheses_are_rejected_rather_than_ignored() {
    // The language has none. Silently dropping them would change what the
    // author wrote into something that evaluates differently.
    assert!(Condition::parse("(branch == 'a')").is_err());
}
