//! Unit tests for the "run with parameters" pieces that need no database: ordering the offered stages,
//! reading what the form submitted, and the form's markup.

use conveyor_service::domain::{Job, Provider, Repo, Status};
use conveyor_service::routers::ui::pages::launch::{launch_form, selected_stages};
use conveyor_service::scheduler::stages::{KnownStage, ordered};
use quench_core::time::Time;

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

fn repo() -> Repo {
    Repo {
        id: "r1".to_string(),
        provider: Provider::Generic,
        owner: "lorehaven".to_string(),
        name: "forge".to_string(),
        clone_url: "https://example.test/forge.git".to_string(),
        default_branch: "master".to_string(),
        registered_by: "admin".to_string(),
        project_id: "p1".to_string(),
        enabled: true,
        created_at: Time::now(),
        updated_at: Time::now(),
    }
}

fn pair(key: &str, value: &str) -> (String, String) {
    (key.to_string(), value.to_string())
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

#[quench_test::test]
fn ticked_boxes_and_typed_extras_are_combined_without_repeats() {
    let pairs = [
        pair("git_ref", "master"),
        pair("stage", "check"),
        pair("stage", "security"),
        pair("extra", " build , check,, lint "),
        pair("unrelated", "x"),
    ];
    assert_eq!(
        selected_stages(&pairs),
        ["check", "security", "build", "lint"]
    );
    assert!(selected_stages(&[pair("git_ref", "x"), pair("extra", " , ")]).is_empty());
}

#[quench_test::test]
fn the_form_ticks_what_an_ordinary_run_does_and_marks_the_rest() {
    let known = [
        KnownStage {
            name: "check".to_string(),
            on_by_default: true,
        },
        KnownStage {
            name: "security".to_string(),
            on_by_default: false,
        },
    ];
    let html = launch_form(&repo(), &known, true).render();

    assert!(
        html.contains("action=\"") && html.contains("/repos/lorehaven/forge/run"),
        "{html}"
    );
    assert!(html.contains("name=\"stage\""));
    // Only `check` starts ticked; `security` is offered and flagged as normally left out.
    assert_eq!(html.matches("checked=\"checked\"").count(), 1, "{html}");
    assert!(html.contains("ui_launch_normally_skipped"));
    assert!(html.contains("name=\"extra\"") && html.contains("name=\"git_ref\""));
    assert!(html.contains("placeholder=\"master\""));
    assert!(!html.contains("disabled"));
}

#[quench_test::test]
fn without_write_access_the_form_is_shown_but_cannot_be_submitted() {
    let html = launch_form(&repo(), &[], false).render();
    assert!(html.contains("disabled"), "{html}");
    assert!(html.contains("ui_launch_no_access"));
    // A repository that has never run offers a way to name stages by hand.
    assert!(html.contains("ui_launch_none_known"));
    assert!(html.contains("name=\"extra\""));
}
