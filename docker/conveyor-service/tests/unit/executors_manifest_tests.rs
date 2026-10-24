//! Unit tests for `executors/manifest.rs`.
//!
//! A cluster is not available here, so what is tested is every decision made
//! before anything is sent to one - which is where the isolation properties
//! live. Running against a real cluster is a manual step, written up in the
//! service README.

use conveyor_service::executors::engine::{JobCredential, JobSpec, SourceSpec};
use conveyor_service::executors::manifest::{self, STEP_MARKER, Settings, WORKSPACE_PATH};
use conveyor_service::pipeline::Step;
use conveyor_service::secrets::Redactor;
use quench_serde::Value;
use std::collections::BTreeMap;
use std::time::Duration;

fn spec() -> JobSpec {
    JobSpec {
        id: "0f8f0e0e-1111-2222-3333-444455556666".to_string(),
        name: "build/cargo".to_string(),
        steps: vec![Step::Run("cargo build".to_string())],
        env: BTreeMap::new(),
        timeout: Duration::from_mins(15),
        image: None,
        source: Some(SourceSpec {
            clone_url: "https://example.invalid/thing.git".to_string(),
            git_ref: "refs/heads/master".to_string(),
            sha: "a".repeat(40),
            credential: None,
        }),
        redactor: Redactor::none(),
    }
}

fn build(spec: &JobSpec) -> manifest::Manifest {
    let commands = spec
        .steps
        .iter()
        .map(|step| conveyor_service::steps::argv(step).expect("resolves"))
        .collect::<Vec<_>>();
    manifest::build(spec, &commands, "conveyor", &Settings::default())
}

// ---------------------------------------------------------------------------
// Names
// ---------------------------------------------------------------------------

#[quench_test::test]
fn an_object_name_is_a_valid_kubernetes_name() {
    let name = manifest::object_name("0F8F0E0E-1111-2222-3333-444455556666");

    assert!(name.starts_with("conveyor-"));
    assert!(name.len() <= 63, "{name} is {} characters", name.len());
    assert!(
        name.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
        "{name}"
    );
    assert!(!name.starts_with('-') && !name.ends_with('-'));
}

#[quench_test::test]
fn an_awkward_id_still_produces_a_usable_name() {
    // Ids are uuids today. This is here so a future scheme cannot silently
    // produce objects the API server rejects.
    for id in ["", "---", "Has Spaces/And.Dots", &"x".repeat(200)] {
        let name = manifest::object_name(id);
        assert!(name.len() <= 63, "{id:?} -> {name}");
        assert!(!name.ends_with('-'), "{id:?} -> {name}");
        assert!(
            name.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
            "{id:?} -> {name}"
        );
    }
}

// ---------------------------------------------------------------------------
// The job
// ---------------------------------------------------------------------------

const POD: &str = "/spec/template/spec";

fn at<'a>(value: &'a Value, pointer: &str) -> &'a Value {
    value
        .pointer(pointer)
        .unwrap_or_else(|| panic!("no {pointer} in {value:?}"))
}

fn text<'a>(value: &'a Value, pointer: &str) -> &'a str {
    at(value, pointer).as_str().expect("a string")
}

fn init(built: &manifest::Manifest) -> &Value {
    at(&built.job, &format!("{POD}/initContainers/0"))
}

#[quench_test::test]
fn the_job_is_labelled_as_conveyors() {
    // What makes a stray job identifiable, and what a cleanup would select on.
    let built = build(&spec());

    assert_eq!(
        text(&built.job, "/metadata/labels/app.kubernetes.io~1managed-by"),
        "conveyor"
    );
    assert_eq!(
        text(&built.job, "/metadata/labels/conveyor.forge~1job-id"),
        "0f8f0e0e-1111-2222-3333-444455556666"
    );
}

#[quench_test::test]
fn the_cluster_never_retries_a_job_itself() {
    // Conveyor owns retries; a silent second attempt inside the cluster would
    // report as one run that took twice as long.
    let built = build(&spec());
    assert_eq!(at(&built.job, "/spec/backoffLimit").as_u64(), Some(0));
}

#[quench_test::test]
fn the_jobs_timeout_becomes_the_clusters_deadline() {
    let built = build(&spec());
    assert_eq!(
        at(&built.job, "/spec/activeDeadlineSeconds").as_u64(),
        Some(900)
    );
}

#[quench_test::test]
fn a_pod_is_never_restarted() {
    let built = build(&spec());
    assert_eq!(text(&built.job, &format!("{POD}/restartPolicy")), "Never");
}

#[quench_test::test]
fn the_steps_run_in_the_image_the_pipeline_named() {
    let mut spec = spec();
    spec.image = Some("rust:1.98".to_string());

    let built = build(&spec);
    assert_eq!(
        text(&built.job, &format!("{POD}/containers/0/image")),
        "rust:1.98"
    );
}

#[quench_test::test]
fn a_pipeline_that_names_no_image_gets_the_deployments_default() {
    let built = build(&spec());
    assert_eq!(
        text(&built.job, &format!("{POD}/containers/0/image")),
        Settings::default().default_image
    );
}

#[quench_test::test]
fn a_step_cannot_gain_privileges_it_was_not_started_with() {
    let built = build(&spec());

    for container in ["containers/0", "initContainers/0"] {
        assert_eq!(
            at(
                &built.job,
                &format!("{POD}/{container}/securityContext/allowPrivilegeEscalation")
            ),
            &Value::Bool(false),
            "{container} may escalate"
        );
    }
}

// ---------------------------------------------------------------------------
// The checkout
// ---------------------------------------------------------------------------

#[quench_test::test]
fn the_pod_fetches_its_own_commit() {
    // Nothing is copied in from conveyor's disk, which is the point of running
    // a fork's pipeline here rather than natively.
    let built = build(&spec());
    assert_eq!(
        at(&built.job, &format!("{POD}/initContainers"))
            .as_array()
            .map(Vec::len),
        Some(1)
    );
    let script = text(init(&built), "/args/0");

    assert!(script.contains("git init"), "{script}");
    assert!(
        script.contains("https://example.invalid/thing.git"),
        "{script}"
    );
    assert!(script.contains(&"a".repeat(40)), "{script}");
    assert!(script.contains("refs/heads/master"), "{script}");
    // The same fallback the local checkout uses: the commit directly, then the
    // ref in full for a server that will not serve a sha.
    assert!(script.contains("--depth 1"), "{script}");
    assert!(script.contains("||"), "{script}");
}

#[quench_test::test]
fn the_checkout_never_waits_for_a_password() {
    // There is no terminal to answer on; the pod would hang to its deadline.
    let built = build(&spec());
    let script = text(init(&built), "/args/0");
    assert!(script.contains("GIT_TERMINAL_PROMPT=0"), "{script}");
}

#[quench_test::test]
fn both_containers_share_the_checkout() {
    let built = build(&spec());

    let mounts = |container: &Value| {
        at(container, "/volumeMounts")
            .as_array()
            .expect("mounts")
            .iter()
            .any(|m| m.get("mountPath").and_then(Value::as_str) == Some(WORKSPACE_PATH))
    };

    let work = at(&built.job, &format!("{POD}/containers/0"));
    assert!(mounts(work), "the work container");
    assert!(mounts(init(&built)), "the init container");
    assert_eq!(text(work, "/workingDir"), WORKSPACE_PATH);
}

#[quench_test::test]
fn a_job_with_no_source_gets_no_init_container() {
    let mut spec = spec();
    spec.source = None;

    let built = build(&spec);
    assert!(
        built
            .job
            .pointer(&format!("{POD}/initContainers"))
            .is_none()
    );
}

#[quench_test::test]
fn a_credential_reaches_the_checkout_container_as_a_git_config_header() {
    // The same mechanism `workspace::checkout` uses for the local clone:
    // `http.extraheader` supplied through `GIT_CONFIG_*` env, never embedded
    // in the checkout script's own text.
    let mut spec = spec();
    spec.source.as_mut().unwrap().credential = Some(JobCredential {
        username: "x-access-token".to_string(),
        token: "s3cr3t-token".to_string(),
    });

    let built = build(&spec);
    let init = init(&built);

    let value_of = |name: &str| {
        at(init, "/env")
            .as_array()
            .expect("env")
            .iter()
            .find(|var| var.get("name").and_then(Value::as_str) == Some(name))
            .and_then(|var| var.get("value")?.as_str())
            .map(str::to_string)
    };

    assert_eq!(value_of("GIT_CONFIG_COUNT").as_deref(), Some("1"));
    assert_eq!(
        value_of("GIT_CONFIG_KEY_0").as_deref(),
        Some("http.extraheader")
    );
    let header = value_of("GIT_CONFIG_VALUE_0").expect("a header value");
    assert!(header.starts_with("Authorization: Basic "), "{header}");

    // Never in the script's own text - that would put it in argv, and a
    // Job's pod spec is retained by the API server for its TTL.
    let script = text(init, "/args/0");
    assert!(!script.contains("s3cr3t-token"), "{script}");

    // Never on the step container - only the checkout container clones the
    // repo; the pipeline's own commands get a job's declared secrets and
    // nothing else.
    assert!(
        built
            .job
            .pointer(&format!("{POD}/containers/0/env"))
            .is_none()
    );
}

#[quench_test::test]
fn no_credential_means_no_extra_env_on_the_checkout_container() {
    let built = build(&spec());
    assert!(init(&built).get("env").is_none());
}

// ---------------------------------------------------------------------------
// Secrets
// ---------------------------------------------------------------------------

#[quench_test::test]
fn a_job_with_no_secrets_creates_no_secret() {
    let built = build(&spec());
    assert!(built.secret.is_none());
    assert!(
        built
            .job
            .pointer(&format!("{POD}/containers/0/envFrom"))
            .is_none()
    );
}

#[quench_test::test]
fn secrets_go_in_a_secret_rather_than_the_pod_spec() {
    // Anyone who can read pods can read an inline env var.
    let mut spec = spec();
    spec.env
        .insert("DEPLOY_TOKEN".to_string(), "s3cret-value".to_string());

    let built = build(&spec);
    let secret = built.secret.expect("a secret");
    assert_eq!(text(&secret, "/stringData/DEPLOY_TOKEN"), "s3cret-value");

    let rendered = quench_serde::json::to_string(&built.job);
    assert!(
        !rendered.contains("s3cret-value"),
        "the value is in the pod spec: {rendered}"
    );
}

#[quench_test::test]
fn the_secret_is_required_rather_than_optional() {
    // Optional would let the pod start without the values and fail somewhere
    // further on, with a blank token.
    let mut spec = spec();
    spec.env.insert("TOKEN".to_string(), "a-value".to_string());

    let built = build(&spec);
    let source = at(
        &built.job,
        &format!("{POD}/containers/0/envFrom/0/secretRef"),
    );

    assert_eq!(at(source, "/optional"), &Value::Bool(false));
    assert_eq!(text(source, "/name"), built.name);
}

// ---------------------------------------------------------------------------
// The script
// ---------------------------------------------------------------------------

#[quench_test::test]
fn each_step_is_announced_so_the_follower_knows_where_it_is() {
    let script = manifest::script(&[
        vec!["sh".to_string(), "-c".to_string(), "one".to_string()],
        vec!["sh".to_string(), "-c".to_string(), "two".to_string()],
    ]);

    assert!(script.contains(&format!("'{STEP_MARKER}' 0")), "{script}");
    assert!(script.contains(&format!("'{STEP_MARKER}' 1")), "{script}");
    // On stderr, so a step that redirects its own stdout cannot swallow it.
    assert!(script.contains(">&2"), "{script}");
}

#[quench_test::test]
fn the_script_stops_at_the_first_failure_with_that_steps_code() {
    let script = manifest::script(&[vec!["false".to_string()], vec!["true".to_string()]]);
    assert!(script.contains("exit $__status"), "{script}");
}

#[quench_test::test]
fn an_argument_with_a_space_stays_one_argument() {
    let script = manifest::script(&[vec![
        "anvil".to_string(),
        "release".to_string(),
        "--message".to_string(),
        "two words".to_string(),
    ]]);
    assert!(script.contains("'two words'"), "{script}");
}

#[quench_test::test]
fn an_argument_with_a_quote_cannot_break_out_of_the_script() {
    // The one that matters: a value carrying `'; rm -rf /; '` would otherwise
    // end the quoting and become a second command.
    let script = manifest::script(&[vec!["echo".to_string(), "'; rm -rf / ; echo '".to_string()]]);

    assert!(!script.contains("; rm -rf / ;\n"), "{script}");
    assert!(
        script.contains(r"'\''"),
        "expected the quote to be escaped: {script}"
    );
}
