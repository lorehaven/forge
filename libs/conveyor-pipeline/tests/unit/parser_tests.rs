//! Unit tests for `pipeline/parser.rs`.

use conveyor_pipeline::parser::SpecError;
use conveyor_pipeline::spec::Step;
use conveyor_pipeline::{PIPELINE_FILE, parse};

/// The file from conveyor's own README, which is the shape people will copy.
const README_PIPELINE: &str = r#"
on = { push = ["master"], pull_request = ["*"] }

[[stage]]
name = "build"
[[stage.job]]
name  = "cargo"
steps = [{ anvil = "build --all" }]

[[stage]]
name  = "test"
needs = ["build"]
[[stage.job]]
name  = "unit"
steps = [{ anvil = "test --all" }, { run = "cargo fmt --check" }]

[[stage]]
name  = "deploy"
needs = ["test"]
when  = "branch == 'master'"
[[stage.job]]
name    = "k8s"
secrets = ["KUBE_TOKEN"]
steps   = [{ anvil = "docker release-all" }, { riveter = "apply k8s/" }]
"#;

fn error(source: &str) -> SpecError {
    parse(source).expect_err("should not parse")
}

fn message(source: &str) -> String {
    error(source).to_string()
}

// ---------------------------------------------------------------------------
// The happy path
// ---------------------------------------------------------------------------

#[test]
fn the_documented_pipeline_parses() {
    let spec = parse(README_PIPELINE).expect("the README example must parse");

    assert_eq!(spec.stages.len(), 3);
    assert_eq!(spec.job_count(), 3);
    assert_eq!(
        spec.stages_in_order()
            .map(|stage| stage.name.as_str())
            .collect::<Vec<_>>(),
        ["build", "test", "deploy"]
    );

    assert_eq!(spec.on.push, ["master"]);
    assert_eq!(spec.on.pull_request, ["*"]);
    assert!(spec.on.tag.is_empty());

    let deploy = spec.stage("deploy").expect("deploy exists");
    assert_eq!(deploy.needs, ["test"]);
    assert!(deploy.when.is_some());
    assert_eq!(deploy.jobs[0].secrets, ["KUBE_TOKEN"]);
    assert_eq!(
        deploy.jobs[0].steps,
        [
            Step::Anvil("docker release-all".to_string()),
            Step::Riveter("apply k8s/".to_string()),
        ]
    );
}

#[test]
fn the_pipeline_file_is_the_documented_name() {
    assert_eq!(PIPELINE_FILE, ".conveyor.toml");
}

#[test]
fn a_bare_string_step_is_a_shell_command() {
    let spec = parse(
        r#"
[[stage]]
name = "build"
[[stage.job]]
steps = ["make all"]
"#,
    )
    .expect("parses");
    assert_eq!(spec.stages[0].jobs[0].steps, [Step::Run("make all".into())]);
}

#[test]
fn every_step_kind_is_accepted() {
    let spec = parse(
        r#"
[[stage]]
name = "all"
[[stage.job]]
steps = [
  { run = "echo hi" },
  { anvil = "build --all" },
  { riveter = "apply k8s/" },
  { warehouse = "files upload artifacts ./thing" },
]
"#,
    )
    .expect("parses");

    let kinds: Vec<_> = spec.stages[0].jobs[0]
        .steps
        .iter()
        .map(Step::kind)
        .collect();
    assert_eq!(kinds, ["run", "anvil", "riveter", "warehouse"]);
}

#[test]
fn job_options_are_carried_through() {
    let spec = parse(
        r#"
[[stage]]
name = "build"
[[stage.job]]
name      = "cargo"
timeout   = 900
image     = "rust:1.94"
secrets   = ["CARGO_TOKEN"]
artifacts = ["target/release/thing"]
steps     = ["cargo build"]

[stage.job.env]
CARGO_TERM_COLOR = "always"
"#,
    )
    .expect("parses");

    let job = &spec.stages[0].jobs[0];
    assert_eq!(job.timeout, Some(900));
    assert_eq!(job.image.as_deref(), Some("rust:1.94"));
    assert_eq!(job.secrets, ["CARGO_TOKEN"]);
    assert_eq!(job.artifacts, ["target/release/thing"]);
    assert_eq!(
        job.env.get("CARGO_TERM_COLOR").map(String::as_str),
        Some("always")
    );
}

// ---------------------------------------------------------------------------
// Defaults
// ---------------------------------------------------------------------------

#[test]
fn a_pipeline_without_on_builds_every_push() {
    let spec = parse(
        r#"
[[stage]]
name = "build"
[[stage.job]]
steps = ["make"]
"#,
    )
    .expect("parses");

    assert!(spec.on.allows("push", "refs/heads/anything"));
    assert!(!spec.on.allows("pull_request", "refs/heads/anything"));
}

#[test]
fn naming_one_event_turns_the_others_off() {
    // `on = { pull_request = ["*"] }` and still getting every push built is not
    // what it looks like it says.
    let spec = parse(
        r#"
on = { pull_request = ["*"] }

[[stage]]
name = "build"
[[stage.job]]
steps = ["make"]
"#,
    )
    .expect("parses");

    assert!(spec.on.allows("pull_request", "refs/heads/topic"));
    assert!(!spec.on.allows("push", "refs/heads/master"));
}

#[test]
fn a_sole_unnamed_job_takes_the_stages_name() {
    let spec = parse(
        r#"
[[stage]]
name = "build"
[[stage.job]]
steps = ["make"]
"#,
    )
    .expect("parses");
    assert_eq!(spec.stages[0].jobs[0].name, "build");
}

#[test]
fn several_unnamed_jobs_are_numbered_from_one() {
    let spec = parse(
        r#"
[[stage]]
name = "build"
[[stage.job]]
steps = ["make a"]
[[stage.job]]
steps = ["make b"]
"#,
    )
    .expect("parses");

    let names: Vec<_> = spec.stages[0]
        .jobs
        .iter()
        .map(|job| job.name.as_str())
        .collect();
    assert_eq!(names, ["job-1", "job-2"]);
}

#[test]
fn names_are_trimmed() {
    let spec = parse(
        r#"
[[stage]]
name = "  build  "
[[stage.job]]
name = "  cargo  "
steps = ["make"]
"#,
    )
    .expect("parses");
    assert_eq!(spec.stages[0].name, "build");
    assert_eq!(spec.stages[0].jobs[0].name, "cargo");
}

// ---------------------------------------------------------------------------
// Structural mistakes
// ---------------------------------------------------------------------------

#[test]
fn a_pipeline_with_no_stages_is_rejected() {
    assert!(matches!(error(""), SpecError::NoStages));
    assert!(matches!(
        error("on = { push = [\"*\"] }"),
        SpecError::NoStages
    ));
}

#[test]
fn a_stage_with_no_jobs_is_rejected() {
    let error = error("[[stage]]\nname = \"build\"\n");
    assert!(matches!(error, SpecError::NoJobs { .. }), "{error:?}");
}

#[test]
fn a_job_with_no_steps_is_rejected() {
    let error = error(
        r#"
[[stage]]
name = "build"
[[stage.job]]
steps = []
"#,
    );
    assert!(matches!(error, SpecError::NoSteps { .. }), "{error:?}");
}

#[test]
fn an_empty_stage_name_is_rejected() {
    let error = error("[[stage]]\nname = \"  \"\n[[stage.job]]\nsteps = [\"x\"]\n");
    assert!(
        matches!(error, SpecError::EmptyStageName { .. }),
        "{error:?}"
    );
}

#[test]
fn duplicate_job_names_within_a_stage_are_rejected() {
    let error = error(
        r#"
[[stage]]
name = "build"
[[stage.job]]
name = "cargo"
steps = ["a"]
[[stage.job]]
name = "cargo"
steps = ["b"]
"#,
    );
    assert!(matches!(error, SpecError::DuplicateJob { .. }), "{error:?}");
}

#[test]
fn a_needs_on_a_job_is_rejected_with_an_explanation() {
    // Left to `deny_unknown_fields` this reads as a typo rather than as a key
    // that belongs one level up.
    let message = message(
        r#"
[[stage]]
name = "build"
[[stage.job]]
needs = ["other"]
steps = ["a"]
"#,
    );
    assert!(message.contains("stage"), "{message}");
    assert!(message.contains("needs"), "{message}");
}

#[test]
fn a_misspelled_key_is_rejected_rather_than_ignored() {
    // A silently ignored key is a pipeline that quietly does not do what it
    // says: this `step` would have produced a job with no steps.
    assert!(
        parse(
            r#"
[[stage]]
name = "build"
[[stage.job]]
step = ["make"]
"#
        )
        .is_err()
    );
}

#[test]
fn a_zero_timeout_is_rejected() {
    let error = error(
        r#"
[[stage]]
name = "build"
[[stage.job]]
timeout = 0
steps = ["a"]
"#,
    );
    assert!(matches!(error, SpecError::ZeroTimeout { .. }), "{error:?}");
}

#[test]
fn an_empty_secret_name_is_rejected() {
    let error = error(
        r#"
[[stage]]
name = "build"
[[stage.job]]
secrets = ["GOOD", "  "]
steps = ["a"]
"#,
    );
    assert!(
        matches!(error, SpecError::EmptySecretName { .. }),
        "{error:?}"
    );
}

#[test]
fn an_empty_trigger_pattern_is_rejected_with_a_suggestion() {
    let message = message(
        r#"
on = { push = [""] }

[[stage]]
name = "build"
[[stage.job]]
steps = ["a"]
"#,
    );
    assert!(message.contains("\"*\""), "{message}");
}

// ---------------------------------------------------------------------------
// Step mistakes
// ---------------------------------------------------------------------------

#[test]
fn an_unknown_step_kind_lists_the_known_ones() {
    let message = message(
        r#"
[[stage]]
name = "build"
[[stage.job]]
steps = [{ npm = "install" }]
"#,
    );
    assert!(message.contains("npm"), "{message}");
    for kind in Step::KINDS {
        assert!(message.contains(kind), "{message} should mention {kind}");
    }
}

#[test]
fn a_step_naming_two_tools_is_rejected() {
    let message = message(
        r#"
[[stage]]
name = "build"
[[stage.job]]
steps = [{ run = "a", anvil = "b" }]
"#,
    );
    assert!(message.contains("exactly one"), "{message}");
}

#[test]
fn an_empty_command_is_rejected() {
    let message = message(
        r#"
[[stage]]
name = "build"
[[stage.job]]
steps = [{ run = "   " }]
"#,
    );
    assert!(message.contains("empty command"), "{message}");
}

#[test]
fn a_non_string_command_is_rejected() {
    let message = message(
        r#"
[[stage]]
name = "build"
[[stage.job]]
steps = [{ run = ["a", "b"] }]
"#,
    );
    assert!(message.contains("command string"), "{message}");
}

#[test]
fn a_step_error_names_the_stage_job_and_position() {
    let message = message(
        r#"
[[stage]]
name = "build"
[[stage.job]]
name = "cargo"
steps = [{ run = "ok" }, { npm = "install" }]
"#,
    );
    assert!(message.contains("build"), "{message}");
    assert!(message.contains("cargo"), "{message}");
    assert!(message.contains('2'), "{message} should name step 2");
}

// ---------------------------------------------------------------------------
// Conditions and the graph, reported through the parser
// ---------------------------------------------------------------------------

#[test]
fn a_bad_stage_condition_names_the_stage() {
    let message = message(
        r#"
[[stage]]
name = "deploy"
when = "branch = 'master'"
[[stage.job]]
steps = ["a"]
"#,
    );
    assert!(message.contains("deploy"), "{message}");
    assert!(message.contains("'=='"), "{message}");
}

#[test]
fn a_bad_job_condition_names_the_job() {
    let message = message(
        r#"
[[stage]]
name = "deploy"
[[stage.job]]
name = "k8s"
when = "author == 'me'"
steps = ["a"]
"#,
    );
    assert!(message.contains("k8s"), "{message}");
    assert!(message.contains("author"), "{message}");
}

#[test]
fn an_unresolvable_graph_is_a_parse_error() {
    // Not a run that gets three stages in and then discovers the typo.
    let error = error(
        r#"
[[stage]]
name = "build"
[[stage.job]]
steps = ["a"]

[[stage]]
name = "deploy"
needs = ["tset"]
[[stage.job]]
steps = ["b"]
"#,
    );
    assert!(matches!(error, SpecError::Graph(_)), "{error:?}");
}

#[test]
fn a_cycle_is_a_parse_error() {
    let error = error(
        r#"
[[stage]]
name = "a"
needs = ["b"]
[[stage.job]]
steps = ["x"]

[[stage]]
name = "b"
needs = ["a"]
[[stage.job]]
steps = ["y"]
"#,
    );
    assert!(matches!(error, SpecError::Graph(_)), "{error:?}");
}

#[test]
fn a_naming_mistake_is_reported_in_preference_to_the_graph_error_it_caused() {
    // Both are wrong here; the empty command is the one the author can act on.
    let message = message(
        r#"
[[stage]]
name = "build"
[[stage.job]]
steps = [{ run = "" }]

[[stage]]
name = "deploy"
needs = ["nope"]
[[stage.job]]
steps = ["b"]
"#,
    );
    assert!(message.contains("empty command"), "{message}");
}

#[test]
fn malformed_toml_is_reported_as_such() {
    let error = error("[[stage]\nname = \"build\"");
    assert!(matches!(error, SpecError::Toml(_)), "{error:?}");
}
