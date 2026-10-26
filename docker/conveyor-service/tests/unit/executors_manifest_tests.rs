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
        services: Vec::new(),
        exports: Vec::new(),
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

#[quench_test::test]
fn the_script_hands_exports_back_as_marker_lines_even_after_a_failed_step() {
    let dir = quench_core::tmp::TempDir::new().expect("temp dir");
    std::fs::create_dir_all(dir.path().join("out")).expect("mkdir");
    // Long enough to span several lines.
    let body = "x".repeat(10_000);
    std::fs::write(dir.path().join("out/report.txt"), &body).expect("write");

    let script = manifest::script_with_exports(
        &[
            vec!["echo".to_string(), "building".to_string()],
            vec!["false".to_string()],
            vec!["echo".to_string(), "never".to_string()],
        ],
        &["out/report.txt".to_string(), "out/missing.txt".to_string()],
    );
    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg(&script)
        .current_dir(dir.path())
        .output()
        .expect("sh runs");

    // The failed step's status survives the export.
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("building") && !stdout.contains("never"));

    let mut encoded = String::new();
    let mut lines = 0;
    for line in stdout.lines() {
        if let Some(rest) = line.strip_prefix(manifest::FILE_MARKER) {
            let (path, chunk) = rest.split_once('\t').expect("path and chunk");
            assert_eq!(path, "out/report.txt");
            assert!(chunk.len() <= 4096);
            encoded.push_str(chunk);
            lines += 1;
        }
    }
    assert!(lines >= 3, "{lines} lines");
    let decoded = quench_core::base64::STANDARD
        .decode(&encoded)
        .expect("base64");
    assert_eq!(decoded, body.as_bytes());
    assert!(!stdout.contains("missing.txt"));
}

#[quench_test::test]
fn a_script_without_exports_has_no_trap() {
    assert!(!manifest::script(&[vec!["true".to_string()]]).contains("trap"));
}

// ---------------------------------------------------------------------------
// Services
// ---------------------------------------------------------------------------

fn postgres() -> conveyor_service::pipeline::Service {
    conveyor_service::pipeline::Service {
        name: "postgres".to_string(),
        image: "pgvector/pgvector:pg18".to_string(),
        env: BTreeMap::from([("POSTGRES_PASSWORD".to_string(), "postgres".to_string())]),
        args: Vec::new(),
        ready: conveyor_service::pipeline::Readiness::Command("pg_isready -U postgres".to_string()),
    }
}

fn with_services(services: Vec<conveyor_service::pipeline::Service>) -> JobSpec {
    JobSpec { services, ..spec() }
}

fn init_containers(manifest: &manifest::Manifest) -> Vec<Value> {
    manifest.job["spec"]["template"]["spec"]["initContainers"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

#[quench_test::test]
fn a_service_is_a_native_sidecar_that_starts_after_the_checkout() {
    let built = build(&with_services(vec![postgres()]));
    let init = init_containers(&built);

    // the commit is fetched first, so a bad ref does not leave a database running for nothing
    assert_eq!(init.len(), 2);
    assert_eq!(init[0]["name"], "checkout");
    assert!(
        init[0].get("restartPolicy").is_none(),
        "the checkout runs to completion"
    );

    let service = &init[1];
    assert_eq!(service["name"], "postgres");
    assert_eq!(service["image"], "pgvector/pgvector:pg18");
    // `restartPolicy: Always` on an init container is what makes it a sidecar: it keeps running beside
    // the steps and is stopped when they finish, so the Job can complete
    assert_eq!(service["restartPolicy"], "Always");
    assert_eq!(
        service["env"],
        quench_serde::json!([{"name": "POSTGRES_PASSWORD", "value": "postgres"}])
    );
    assert_eq!(
        service["securityContext"]["allowPrivilegeEscalation"],
        false
    );
    assert!(service.get("args").is_none() && service.get("ports").is_none());

    // the steps are still the only regular container
    let containers = built.job["spec"]["template"]["spec"]["containers"]
        .as_array()
        .unwrap();
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0]["name"], "steps");
}

#[quench_test::test]
fn the_steps_wait_for_a_service_by_its_startup_probe() {
    let built = build(&with_services(vec![postgres()]));
    let probe = &init_containers(&built)[1]["startupProbe"];

    assert_eq!(
        probe["exec"]["command"],
        quench_serde::json!(["sh", "-c", "pg_isready -U postgres"])
    );
    assert_eq!(probe["periodSeconds"], 2);
    assert_eq!(probe["timeoutSeconds"], 5);
    // five minutes of checks at the default, so the start is given up on rather than awaited for ever
    assert_eq!(probe["failureThreshold"], 150);
}

#[quench_test::test]
fn the_time_a_service_gets_to_become_ready_is_configurable() {
    let settings = Settings {
        service_ready_timeout_secs: 60,
        ..Settings::default()
    };
    let spec = with_services(vec![postgres()]);
    let commands = vec![conveyor_service::steps::argv(&spec.steps[0]).unwrap()];
    let built = manifest::build(&spec, &commands, "conveyor", &settings);
    assert_eq!(
        init_containers(&built)[1]["startupProbe"]["failureThreshold"],
        30
    );

    // never fewer than one check, whatever the setting
    let settings = Settings {
        service_ready_timeout_secs: 0,
        ..Settings::default()
    };
    let built = manifest::build(&spec, &commands, "conveyor", &settings);
    assert_eq!(
        init_containers(&built)[1]["startupProbe"]["failureThreshold"],
        1
    );
}

#[quench_test::test]
fn a_port_readiness_check_is_a_tcp_probe_and_publishes_the_port() {
    let cache = conveyor_service::pipeline::Service {
        name: "cache".to_string(),
        image: "redis:7".to_string(),
        env: BTreeMap::new(),
        args: vec!["--save".to_string(), String::new()],
        ready: conveyor_service::pipeline::Readiness::Port(6379),
    };
    let built = build(&with_services(vec![cache]));
    let service = &init_containers(&built)[1];

    assert_eq!(service["startupProbe"]["tcpSocket"]["port"], 6379);
    assert!(service["startupProbe"].get("exec").is_none());
    assert_eq!(
        service["ports"],
        quench_serde::json!([{"containerPort": 6379}])
    );
    assert_eq!(service["args"], quench_serde::json!(["--save", ""]));
    assert!(service.get("env").is_none(), "no environment, no env block");
}

#[quench_test::test]
fn several_services_start_in_the_order_they_were_declared() {
    let mut cache = postgres();
    cache.name = "cache".to_string();
    cache.ready = conveyor_service::pipeline::Readiness::Port(6379);
    let built = build(&with_services(vec![postgres(), cache]));

    let names: Vec<_> = init_containers(&built)
        .iter()
        .map(|c| c["name"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(names, ["checkout", "postgres", "cache"]);
}

#[quench_test::test]
fn a_job_without_a_checkout_still_gets_its_services() {
    let mut spec = with_services(vec![postgres()]);
    spec.source = None;
    let commands = vec![conveyor_service::steps::argv(&spec.steps[0]).unwrap()];
    let built = manifest::build(&spec, &commands, "conveyor", &Settings::default());

    let names: Vec<_> = init_containers(&built)
        .iter()
        .map(|c| c["name"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(names, ["postgres"]);
}

#[quench_test::test]
fn a_job_without_services_has_the_pod_it_always_had() {
    let built = build(&spec());
    let init = init_containers(&built);
    assert_eq!(init.len(), 1);
    assert_eq!(init[0]["name"], "checkout");
}

#[quench_test::test]
fn the_steps_secret_is_never_handed_to_a_service_nor_a_services_env_to_the_steps() {
    let mut job = with_services(vec![postgres()]);
    job.env
        .insert("API_TOKEN".to_string(), "hunter2-secret".to_string());
    let built = build(&job);

    // the job's environment travels in the Secret, which only the steps container reads
    let secret = built.secret.as_ref().expect("a secret");
    assert!(secret["stringData"].get("API_TOKEN").is_some());
    assert!(secret["stringData"].get("POSTGRES_PASSWORD").is_none());
    let rendered_service = quench_serde::json::to_string(&init_containers(&built)[1]);
    assert!(
        !rendered_service.contains("hunter2-secret"),
        "{rendered_service}"
    );
    assert!(
        !rendered_service.contains("secretRef"),
        "{rendered_service}"
    );
    assert!(
        built.job["spec"]["template"]["spec"]["containers"][0]["envFrom"].is_array(),
        "the steps container still reads the Secret"
    );
}

#[quench_test::test]
fn the_deployment_can_tune_how_long_pods_and_services_get() {
    let _env = crate::env_support::kubernetes_env();
    let defaults = Settings::from_env();
    assert_eq!(
        (
            defaults.pod_start_timeout_secs,
            defaults.service_ready_timeout_secs
        ),
        (120, 300)
    );

    quench_core::env::set("CONVEYOR_K8S_POD_START_SECS", "45");
    quench_core::env::set("CONVEYOR_K8S_SERVICE_READY_SECS", "90");
    let tuned = Settings::from_env();
    quench_core::env::set("CONVEYOR_K8S_POD_START_SECS", "soon");
    let garbled = Settings::from_env();
    quench_core::env::remove("CONVEYOR_K8S_POD_START_SECS");
    quench_core::env::remove("CONVEYOR_K8S_SERVICE_READY_SECS");

    assert_eq!(
        (
            tuned.pod_start_timeout_secs,
            tuned.service_ready_timeout_secs
        ),
        (45, 90)
    );
    assert_eq!(
        garbled.pod_start_timeout_secs, 120,
        "an unreadable value falls back"
    );
}

// ---------------------------------------------------------------------------
// This repository's own pipeline, as a pod
// ---------------------------------------------------------------------------

/// The repository's `coverage` job, from the file on disk to the Job conveyor would create: the parser, the
/// job as the scheduler hands it to an executor, and the manifest.
#[quench_test::test]
fn the_coverage_job_of_this_repository_becomes_a_pod_with_a_database_beside_its_steps() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.conveyor.toml");
    let source = std::fs::read_to_string(&path).expect("the repository's .conveyor.toml");
    let pipeline = conveyor_service::pipeline::parse(&source).expect("it parses");
    let job = pipeline
        .stages
        .iter()
        .flat_map(|stage| &stage.jobs)
        .find(|job| job.name == "coverage")
        .expect("a coverage job");

    let spec = JobSpec {
        id: "0f8f0e0e-1111-2222-3333-444455556666".to_string(),
        name: "test/coverage".to_string(),
        steps: job.steps.clone(),
        env: job.env.clone(),
        timeout: Duration::from_mins(60),
        image: job.image.clone(),
        source: spec().source,
        redactor: Redactor::none(),
        services: job.services.clone(),
        exports: Vec::new(),
    };
    let built = build(&spec);
    let pod = &built.job["spec"]["template"]["spec"];

    // the checkout, then the database, then the steps
    let init: Vec<_> = init_containers(&built)
        .iter()
        .map(|c| c["name"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(init, ["checkout", "postgres"]);
    let database = &init_containers(&built)[1];
    assert_eq!(database["restartPolicy"], "Always");
    assert_eq!(database["image"], "pgvector/pgvector:pg18");
    assert_eq!(
        database["startupProbe"]["exec"]["command"][2],
        "pg_isready -h 127.0.0.1 -U postgres"
    );
    assert_eq!(pod["containers"].as_array().unwrap().len(), 1);

    // every service's test database URL reaches the steps through the Secret, which the steps container
    // reads whole - not as environment entries written into the pod spec
    let secret = built
        .secret
        .as_ref()
        .expect("the job's environment is a Secret");
    for service in [
        "CONVEYOR",
        "DAYBOOK",
        "FOUNDRY",
        "GANTRY",
        "GATEHOUSE",
        "SAGE",
        "WAREHOUSE",
        "WORKBENCH",
    ] {
        let key = format!("{service}_TEST_DATABASE_URL");
        let url = secret["stringData"][&key]
            .as_str()
            .unwrap_or_else(|| panic!("{key} is not in the Secret"));
        assert!(url.contains("@127.0.0.1:5432/"), "{key}: {url}");
    }
    let steps = &pod["containers"][0];
    assert!(
        steps.get("env").is_none(),
        "no inline environment on the steps container"
    );
    assert!(steps["envFrom"][0]["secretRef"]["name"].is_string());

    // the schema is applied before the tests, by the first step
    let script = steps["args"][0].as_str().expect("the steps script");
    let apply = script.find("foundry-service").expect("the apply step");
    let coverage = script
        .find("cargo llvm-cov --workspace")
        .expect("the coverage step");
    assert!(
        apply < coverage,
        "the database is filled before the tests run"
    );
}
