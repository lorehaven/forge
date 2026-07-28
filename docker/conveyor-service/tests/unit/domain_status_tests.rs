//! Unit tests for `domain/status.rs`.

use conveyor_service::domain::Status;

#[test]
fn parses_every_status_it_renders() {
    for status in [
        Status::Queued,
        Status::Running,
        Status::Success,
        Status::Failed,
        Status::Cancelled,
        Status::Skipped,
    ] {
        assert_eq!(Status::parse(status.as_str()), Some(status));
    }
}

#[test]
fn parsing_ignores_case_and_surrounding_space() {
    assert_eq!(Status::parse("  SUCCESS "), Some(Status::Success));
    assert_eq!(Status::parse("Running"), Some(Status::Running));
}

#[test]
fn parsing_rejects_anything_else() {
    // A status column holding a value we do not recognise is a bug worth
    // surfacing, not something to coerce into `Queued`.
    assert_eq!(Status::parse("passed"), None);
    assert_eq!(Status::parse(""), None);
}

#[test]
fn only_resting_states_are_terminal() {
    assert!(!Status::Queued.is_terminal());
    assert!(!Status::Running.is_terminal());
    assert!(Status::Success.is_terminal());
    assert!(Status::Failed.is_terminal());
    assert!(Status::Cancelled.is_terminal());
    assert!(Status::Skipped.is_terminal());
}

#[test]
fn skipped_is_not_a_failure() {
    // A stage whose `when` excluded it did what the pipeline asked for; if this
    // ever flips, every conditional deploy stage reports the build as broken.
    assert!(!Status::Skipped.is_failure());
    assert!(!Status::Success.is_failure());
    assert!(Status::Failed.is_failure());
    assert!(Status::Cancelled.is_failure());
}

#[test]
fn rollup_keeps_the_parent_running_while_anything_still_moves() {
    assert_eq!(
        Status::rollup([Status::Success, Status::Running]),
        Status::Running
    );
    // Queued counts as still moving: the job has not been given its answer yet.
    assert_eq!(
        Status::rollup([Status::Failed, Status::Queued]),
        Status::Running
    );
}

#[test]
fn rollup_reports_failure_over_cancellation() {
    assert_eq!(
        Status::rollup([Status::Cancelled, Status::Failed, Status::Success]),
        Status::Failed
    );
}

#[test]
fn rollup_reports_cancellation_over_success() {
    assert_eq!(
        Status::rollup([Status::Success, Status::Cancelled]),
        Status::Cancelled
    );
}

#[test]
fn rollup_is_success_only_when_something_succeeded_and_nothing_went_wrong() {
    assert_eq!(
        Status::rollup([Status::Success, Status::Skipped, Status::Success]),
        Status::Success
    );
}

#[test]
fn a_parent_whose_children_all_skipped_is_skipped() {
    // Not a hollow success: nothing was verified, and reporting green would
    // make a pipeline that excluded every stage look like a passing build.
    assert_eq!(
        Status::rollup([Status::Skipped, Status::Skipped]),
        Status::Skipped
    );
}

#[test]
fn a_parent_with_no_children_is_skipped() {
    assert_eq!(Status::rollup([]), Status::Skipped);
}
