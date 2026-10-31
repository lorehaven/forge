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

/// The pipeline this repository actually runs.
const FORGE: &str = include_str!("../../../../.conveyor.toml");

fn will_run(spec: &conveyor_pipeline::PipelineSpec, event: &str, git_ref: &str) -> Vec<String> {
    plan(spec, &EvalContext::new(event, git_ref, "abc"))
        .into_iter()
        .filter(|stage| stage.decision.will_run())
        .map(|stage| spec.stages[stage.index].name.clone())
        .collect()
}

#[quench_test::test]
fn the_release_stages_of_forges_pipeline_never_run_by_themselves() {
    let spec = parse(FORGE).unwrap();
    for (event, git_ref) in [
        ("push", "refs/heads/master"),
        ("pull_request", "refs/heads/feature"),
        ("manual", "master"),
        ("schedule", "refs/heads/master"),
    ] {
        let running = will_run(&spec, event, git_ref);
        assert!(
            !running.iter().any(|s| s.starts_with("release")),
            "{event} on {git_ref} ran {running:?}"
        );
    }
    // Ordinary runs are what they were.
    assert_eq!(
        will_run(&spec, "push", "refs/heads/master"),
        ["check", "build", "test"]
    );
    assert_eq!(
        will_run(&spec, "schedule", "refs/heads/master"),
        ["security"]
    );
}

#[quench_test::test]
fn naming_the_release_stages_runs_them_crates_first() {
    let spec = parse(FORGE).unwrap();
    let only = spec
        .only(&names(&["release-artifacts", "release-crates"]))
        .unwrap();
    assert_eq!(
        will_run(&only, "manual", "master"),
        ["release-crates", "release-artifacts"],
        "the artifacts follow the crates whatever order they are named in"
    );
    // With the tests of the same run first: release only after they pass.
    let all = spec
        .only(&names(&[
            "check",
            "build",
            "test",
            "release-crates",
            "release-artifacts",
        ]))
        .unwrap();
    let order: Vec<&str> = all.stages_in_order().map(|s| s.name.as_str()).collect();
    assert_eq!(
        order,
        [
            "check",
            "build",
            "test",
            "release-crates",
            "release-artifacts"
        ]
    );
    // The crates alone, as a developer's `anvil release --all` would.
    let crates = spec.only(&names(&["release-crates"])).unwrap();
    assert_eq!(will_run(&crates, "manual", "master"), ["release-crates"]);
}

#[quench_test::test]
fn the_release_jobs_ask_for_the_secrets_they_need() {
    let spec = parse(FORGE).unwrap();
    let job = |stage: &str| spec.stage(stage).unwrap().jobs[0].clone();
    for stage in ["release-crates", "release-artifacts"] {
        let secrets = &job(stage).secrets;
        for needed in [
            "RELEASE_GIT_TOKEN",
            "ANVIL_APP_TOKEN",
            "ANVIL_GATEHOUSE_URL",
        ] {
            assert!(
                secrets.iter().any(|s| s == needed),
                "{stage} lacks {needed}"
            );
        }
    }
    let artifacts = job("release-artifacts").secrets;
    for needed in [
        "QUENCH_KEYSTORE_PASSWORD",
        "QUENCH_KEY_PASSWORD",
        "HOIST_KEYSTORE_B64",
        "DAYBOOK_KEYSTORE_B64",
        "FORKLIFT_KEYSTORE_B64",
    ] {
        assert!(
            artifacts.iter().any(|s| s == needed),
            "artifacts lacks {needed}"
        );
    }
}

#[quench_test::test]
fn the_crates_release_has_a_docker_daemon_and_only_that_container_is_privileged() {
    let spec = parse(FORGE).unwrap();
    let job = &spec.stage("release-crates").unwrap().jobs[0];
    let privileged: Vec<&str> = job
        .services
        .iter()
        .filter(|s| s.privileged)
        .map(|s| s.name.as_str())
        .collect();
    assert_eq!(privileged, ["dind"]);
    assert_eq!(
        job.env.get("DOCKER_HOST").map(String::as_str),
        Some("tcp://127.0.0.1:2375")
    );
    for needed in ["DOCKER_REGISTRY_USER", "DOCKER_REGISTRY_PASSWORD"] {
        assert!(job.secrets.iter().any(|s| s == needed), "lacks {needed}");
    }
    // No other job of the pipeline asks for a privileged container.
    let others = spec
        .stages
        .iter()
        .flat_map(|stage| &stage.jobs)
        .filter(|j| j.name != "crates")
        .flat_map(|j| &j.services)
        .filter(|s| s.privileged)
        .count();
    assert_eq!(others, 0);
}
