//! Unit tests for the "run with parameters" pieces that need no database: ordering the offered stages.
//! (Reading the form is `conveyor-core`'s `LaunchDraft`.)

use conveyor_service::domain::{Job, Status};
use conveyor_service::scheduler::stages::ordered;

fn job(stage: &str, needs: &[&str]) -> Job {
    Job {
        id: format!("job-{stage}"),
        run_id: "run".to_string(),
        stage: stage.to_string(),
        name: "j".to_string(),
        needs: needs.iter().map(ToString::to_string).collect(),
        status: Status::Success,
        exit_code: None,
        started_at: None,
        finished_at: None,
        error: None,
        reused_from_run: None,
    }
}

#[quench_test::test]
fn stages_are_offered_dependencies_first_and_ties_by_name() {
    // Jobs arrive in whatever order the database returns them.
    let jobs = [
        job("test", &["build"]),
        job("security", &[]),
        job("check", &[]),
        job("build", &["check"]),
    ];
    assert_eq!(ordered(&jobs), ["check", "build", "security", "test"]);
    // Several jobs in one stage are one stage.
    let more = [job("a", &[]), job("a", &[]), job("b", &["a"])];
    assert_eq!(ordered(&more), ["a", "b"]);
}

#[quench_test::test]
fn a_need_that_is_not_in_the_run_or_a_cycle_cannot_hide_a_stage() {
    assert_eq!(ordered(&[job("b", &["gone"]), job("a", &[])]), ["a", "b"]);
    let cycle = ordered(&[job("x", &["y"]), job("y", &["x"])]);
    assert_eq!(cycle.len(), 2, "{cycle:?}");
    assert!(ordered(&[]).is_empty());
}
