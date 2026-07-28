//! Unit tests for `executors/manifest.rs`.
//!
//! A cluster is not available here, so what is tested is every decision made
//! before anything is sent to one - which is where the isolation properties
//! live. Running against a real cluster is a manual step, written up in the
//! service README.

use conveyor_service::executors::engine::{JobSpec, SourceSpec};
use conveyor_service::executors::manifest::{self, STEP_MARKER, Settings, WORKSPACE_PATH};
use conveyor_service::pipeline::Step;
use conveyor_service::secrets::Redactor;
use std::collections::BTreeMap;
use std::time::Duration;

fn spec() -> JobSpec {
    JobSpec {
        id: "0f8f0e0e-1111-2222-3333-444455556666".to_string(),
        name: "build/cargo".to_string(),
        steps: vec![Step::Run("cargo build".to_string())],
        env: BTreeMap::new(),
        timeout: Duration::from_secs(900),
        image: None,
        source: Some(SourceSpec {
            clone_url: "https://example.invalid/thing.git".to_string(),
            git_ref: "refs/heads/master".to_string(),
            sha: "a".repeat(40),
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

#[test]
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

#[test]
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

#[test]
fn the_job_is_labelled_as_conveyors() {
    // What makes a stray job identifiable, and what a cleanup would select on.
    let built = build(&spec());
    let labels = built.job.metadata.labels.expect("labels");

    assert_eq!(
        labels
            .get("app.kubernetes.io/managed-by")
            .map(String::as_str),
        Some("conveyor")
    );
    assert_eq!(
        labels.get("conveyor.forge/job-id").map(String::as_str),
        Some("0f8f0e0e-1111-2222-3333-444455556666")
    );
}

#[test]
fn the_cluster_never_retries_a_job_itself() {
    // Conveyor owns retries; a silent second attempt inside the cluster would
    // report as one run that took twice as long.
    let built = build(&spec());
    let job = built.job.spec.expect("spec");
    assert_eq!(job.backoff_limit, Some(0));
}

#[test]
fn the_jobs_timeout_becomes_the_clusters_deadline() {
    let built = build(&spec());
    let job = built.job.spec.expect("spec");
    assert_eq!(job.active_deadline_seconds, Some(900));
}

#[test]
fn a_pod_is_never_restarted() {
    let built = build(&spec());
    let pod = built.job.spec.unwrap().template.spec.expect("pod spec");
    assert_eq!(pod.restart_policy.as_deref(), Some("Never"));
}

#[test]
fn the_steps_run_in_the_image_the_pipeline_named() {
    let mut spec = spec();
    spec.image = Some("rust:1.94".to_string());

    let built = build(&spec);
    let pod = built.job.spec.unwrap().template.spec.expect("pod spec");
    assert_eq!(pod.containers[0].image.as_deref(), Some("rust:1.94"));
}

#[test]
fn a_pipeline_that_names_no_image_gets_the_deployments_default() {
    let built = build(&spec());
    let pod = built.job.spec.unwrap().template.spec.expect("pod spec");
    assert_eq!(
        pod.containers[0].image.as_deref(),
        Some(Settings::default().default_image.as_str())
    );
}

#[test]
fn a_step_cannot_gain_privileges_it_was_not_started_with() {
    let built = build(&spec());
    let pod = built.job.spec.unwrap().template.spec.expect("pod spec");

    for container in pod
        .containers
        .iter()
        .chain(pod.init_containers.iter().flatten())
    {
        assert_eq!(
            container
                .security_context
                .as_ref()
                .and_then(|c| c.allow_privilege_escalation),
            Some(false),
            "{} may escalate",
            container.name
        );
    }
}

// ---------------------------------------------------------------------------
// The checkout
// ---------------------------------------------------------------------------

#[test]
fn the_pod_fetches_its_own_commit() {
    // Nothing is copied in from conveyor's disk, which is the point of running
    // a fork's pipeline here rather than natively.
    let built = build(&spec());
    let pod = built.job.spec.unwrap().template.spec.expect("pod spec");
    let init = pod.init_containers.expect("an init container");

    assert_eq!(init.len(), 1);
    let script = init[0].args.as_ref().expect("args")[0].clone();

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

#[test]
fn the_checkout_never_waits_for_a_password() {
    // There is no terminal to answer on; the pod would hang to its deadline.
    let built = build(&spec());
    let pod = built.job.spec.unwrap().template.spec.expect("pod spec");
    let script = pod.init_containers.unwrap()[0].args.as_ref().unwrap()[0].clone();
    assert!(script.contains("GIT_TERMINAL_PROMPT=0"), "{script}");
}

#[test]
fn both_containers_share_the_checkout() {
    let built = build(&spec());
    let pod = built.job.spec.unwrap().template.spec.expect("pod spec");

    let mounts = |container: &k8s_openapi::api::core::v1::Container| {
        container
            .volume_mounts
            .as_ref()
            .map(|mounts| mounts.iter().any(|m| m.mount_path == WORKSPACE_PATH))
            .unwrap_or(false)
    };

    assert!(mounts(&pod.containers[0]), "the work container");
    assert!(
        mounts(&pod.init_containers.unwrap()[0]),
        "the init container"
    );
    assert_eq!(
        pod.containers[0].working_dir.as_deref(),
        Some(WORKSPACE_PATH)
    );
}

#[test]
fn a_job_with_no_source_gets_no_init_container() {
    let mut spec = spec();
    spec.source = None;

    let built = build(&spec);
    let pod = built.job.spec.unwrap().template.spec.expect("pod spec");
    assert!(pod.init_containers.is_none());
}

// ---------------------------------------------------------------------------
// Secrets
// ---------------------------------------------------------------------------

#[test]
fn a_job_with_no_secrets_creates_no_secret() {
    let built = build(&spec());
    assert!(built.secret.is_none());

    let pod = built.job.spec.unwrap().template.spec.expect("pod spec");
    assert!(pod.containers[0].env_from.is_none());
}

#[test]
fn secrets_go_in_a_secret_rather_than_the_pod_spec() {
    // Anyone who can read pods can read an inline env var.
    let mut spec = spec();
    spec.env
        .insert("DEPLOY_TOKEN".to_string(), "s3cret-value".to_string());

    let built = build(&spec);
    let secret = built.secret.expect("a secret");
    assert_eq!(
        secret
            .string_data
            .expect("data")
            .get("DEPLOY_TOKEN")
            .map(String::as_str),
        Some("s3cret-value")
    );

    let rendered = serde_json::to_string(&built.job).expect("serialise");
    assert!(
        !rendered.contains("s3cret-value"),
        "the value is in the pod spec: {rendered}"
    );
}

#[test]
fn the_secret_is_required_rather_than_optional() {
    // Optional would let the pod start without the values and fail somewhere
    // further on, with a blank token.
    let mut spec = spec();
    spec.env.insert("TOKEN".to_string(), "a-value".to_string());

    let built = build(&spec);
    let pod = built.job.spec.unwrap().template.spec.expect("pod spec");
    let source = pod.containers[0].env_from.as_ref().expect("env_from")[0]
        .secret_ref
        .clone()
        .expect("secret ref");

    assert_eq!(source.optional, Some(false));
    assert_eq!(source.name, built.name);
}

// ---------------------------------------------------------------------------
// The script
// ---------------------------------------------------------------------------

#[test]
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

#[test]
fn the_script_stops_at_the_first_failure_with_that_steps_code() {
    let script = manifest::script(&[vec!["false".to_string()], vec!["true".to_string()]]);
    assert!(script.contains("exit $__status"), "{script}");
}

#[test]
fn an_argument_with_a_space_stays_one_argument() {
    let script = manifest::script(&[vec![
        "anvil".to_string(),
        "release".to_string(),
        "--message".to_string(),
        "two words".to_string(),
    ]]);
    assert!(script.contains("'two words'"), "{script}");
}

#[test]
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
