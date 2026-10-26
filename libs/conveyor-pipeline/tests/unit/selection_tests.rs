//! Unit tests for `PipelineSpec::only`: running a chosen subset of a pipeline's stages by hand.

use conveyor_pipeline::{EvalContext, parse, plan};

const PIPELINE: &str = r#"
on = { push = ["master"], schedule = ["0 0 * * *"] }

[[stage]]
name = "check"
when = "event != 'schedule'"
[[stage.job]]
name = "lint"
steps = [{ run = "true" }]

[[stage]]
name  = "build"
needs = ["check"]
when  = "event != 'schedule'"
[[stage.job]]
name = "workspace"
steps = [{ run = "true" }]

[[stage]]
name = "security"
when = "event == 'schedule'"
[[stage.job]]
name = "nightly"
steps = [{ run = "true" }]

[[stage]]
name  = "deploy"
needs = ["build"]
[[stage.job]]
name = "release"
when = "branch == 'master'"
steps = [{ run = "true" }]
"#;

fn names(values: &[&str]) -> Vec<String> {
    values.iter().map(ToString::to_string).collect()
}

fn runs(spec: &conveyor_pipeline::PipelineSpec, branch: &str) -> Vec<String> {
    plan(spec, &EvalContext::new("manual", branch, "abc"))
        .into_iter()
        .filter(|stage| stage.decision.will_run())
        .map(|stage| spec.stages[stage.index].name.clone())
        .collect()
}

#[quench_test::test]
fn a_stage_that_only_runs_on_a_schedule_runs_when_it_is_asked_for_by_name() {
    let spec = parse(PIPELINE).unwrap();
    // A plain manual run does not include the nightly stage ...
    assert!(!runs(&spec, "master").contains(&"security".to_string()));
    // ... naming it is how it is run by hand, on its own.
    let only = spec.only(&names(&["security"])).unwrap();
    assert_eq!(runs(&only, "master"), ["security"]);
}

#[quench_test::test]
fn stages_can_be_run_together_in_declaration_order() {
    let spec = parse(PIPELINE).unwrap();
    // The caller's order is irrelevant; the pipeline's is kept.
    let only = spec.only(&names(&["security", "check"])).unwrap();
    assert_eq!(runs(&only, "master"), ["check", "security"]);
}

#[quench_test::test]
fn needs_on_a_stage_that_was_not_selected_do_not_block() {
    let spec = parse(PIPELINE).unwrap();
    // `build` needs `check`; selecting `build` alone runs it, and does not run `check`.
    let only = spec.only(&names(&["build"])).unwrap();
    assert_eq!(runs(&only, "master"), ["build"]);

    // Needs between selected stages are kept: deploy still follows build.
    let both = spec.only(&names(&["deploy", "build"])).unwrap();
    assert_eq!(runs(&both, "master"), ["build", "deploy"]);
    let order: Vec<&str> = both.stages_in_order().map(|s| s.name.as_str()).collect();
    assert_eq!(order, ["build", "deploy"]);
}

#[quench_test::test]
fn a_jobs_own_condition_still_applies_inside_a_selected_stage() {
    let spec = parse(PIPELINE).unwrap();
    let only = spec.only(&names(&["deploy"])).unwrap();
    let on_master = plan(&only, &EvalContext::new("manual", "master", "abc"));
    assert!(on_master[0].jobs[0].decision.will_run());
    // The stage was forced on, but its job's guard is a finer one and holds: no release from a feature branch.
    let on_feature = plan(&only, &EvalContext::new("manual", "feature", "abc"));
    assert!(!on_feature[0].jobs[0].decision.will_run());
}

#[quench_test::test]
fn the_original_pipeline_is_left_alone() {
    let spec = parse(PIPELINE).unwrap();
    let before = runs(&spec, "master");
    let _ = spec.only(&names(&["security"])).unwrap();
    assert_eq!(runs(&spec, "master"), before);
    assert_eq!(spec.stages.len(), 4);
    assert_eq!(spec.schedules.len(), 1);
}

#[quench_test::test]
fn naming_nothing_or_a_stage_that_does_not_exist_says_what_there_is() {
    let spec = parse(PIPELINE).unwrap();

    let error = spec.only(&names(&["securty", "check"])).unwrap_err();
    assert_eq!(error.unknown, ["securty"]);
    assert_eq!(error.available, ["check", "build", "security", "deploy"]);
    let message = error.to_string();
    assert!(
        message.contains("securty") && message.contains("check, build, security, deploy"),
        "{message}"
    );

    for empty in [names(&[]), names(&["", "  "])] {
        let error = spec.only(&empty).unwrap_err();
        assert!(error.unknown.is_empty());
        assert!(
            error.to_string().contains("no stages were selected"),
            "{error}"
        );
    }
}

#[quench_test::test]
fn names_are_trimmed_and_repeats_are_one_stage() {
    let spec = parse(PIPELINE).unwrap();
    let only = spec
        .only(&names(&[" check ", "check", "security"]))
        .unwrap();
    assert_eq!(only.stages.len(), 2);
}
