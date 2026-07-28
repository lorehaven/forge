//! Unit tests for `secrets/redact.rs`.

use conveyor_service::secrets::Redactor;
use conveyor_service::secrets::redact::MIN_REDACTABLE;

#[test]
fn a_secret_is_replaced_wherever_it_appears() {
    let redactor = Redactor::new(["hunter2-token"]);
    let redacted = redactor.apply("Authorization: Bearer hunter2-token");

    assert!(!redacted.contains("hunter2-token"));
    assert!(redacted.starts_with("Authorization: Bearer "));
}

#[test]
fn every_occurrence_on_a_line_goes() {
    let redactor = Redactor::new(["secret-value"]);
    let redacted = redactor.apply("secret-value and again secret-value");
    assert!(!redactor.leaks(&redacted));
}

#[test]
fn several_secrets_are_all_replaced() {
    let redactor = Redactor::new(["first-secret", "second-secret"]);
    let redacted = redactor.apply("first-secret then second-secret");
    assert!(!redactor.leaks(&redacted));
}

#[test]
fn a_secret_containing_another_is_replaced_whole() {
    // Longest first. The other order masks the inner value and leaves the rest
    // of the outer one - half of a token in the log, which is worse than
    // useless because it looks redacted.
    let redactor = Redactor::new(["token", "token-with-suffix"]);
    let redacted = redactor.apply("value=token-with-suffix");

    assert!(!redacted.contains("token-with-suffix"));
    assert!(
        !redacted.contains("-with-suffix"),
        "the remainder of the longer secret survived: {redacted}"
    );
}

#[test]
fn text_that_holds_no_secret_is_untouched() {
    let redactor = Redactor::new(["a-secret-value"]);
    let line = "Compiling conveyor-service v0.1.0";
    assert_eq!(redactor.apply(line), line);
}

#[test]
fn a_redactor_with_nothing_to_hide_changes_nothing() {
    let redactor = Redactor::none();
    assert!(redactor.is_empty());
    assert_eq!(redactor.apply("anything at all"), "anything at all");
}

#[test]
fn very_short_values_are_not_redacted() {
    // Replacing every `ab` in a build log destroys the log and still tells
    // anyone reading it what the value was. The store refuses to hold one, so
    // a redactor should never see it - this is the belt to that braces.
    let redactor = Redactor::new(["ab", "x"]);
    assert!(redactor.is_empty());
    assert_eq!(redactor.apply("abxabx"), "abxabx");
}

#[test]
fn a_value_at_the_threshold_is_redacted() {
    let value = "a".repeat(MIN_REDACTABLE);
    let redactor = Redactor::new([value.clone()]);

    assert!(!redactor.is_empty());
    assert!(!redactor.leaks(&redactor.apply(&format!("token={value}"))));
}

#[test]
fn duplicate_values_are_handled_once() {
    let redactor = Redactor::new(["same-value", "same-value"]);
    assert!(!redactor.leaks(&redactor.apply("same-value")));
}

#[test]
fn multiline_output_is_redacted_throughout() {
    let redactor = Redactor::new(["deploy-token"]);
    let redacted = redactor.apply("setting up\nTOKEN=deploy-token\ndone");
    assert!(!redactor.leaks(&redacted));
    assert!(redacted.contains("setting up") && redacted.contains("done"));
}

#[test]
fn redaction_is_a_backstop_not_a_guarantee() {
    // Written down because it is the honest limit: a step that transforms a
    // secret before printing it gets past this, and nothing short of not
    // injecting the secret would stop it.
    let redactor = Redactor::new(["plain-secret"]);
    let encoded: String = "plain-secret".chars().rev().collect();
    assert_eq!(redactor.apply(&encoded), encoded);
}
