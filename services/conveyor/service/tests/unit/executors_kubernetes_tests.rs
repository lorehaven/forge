//! `executors/kubernetes.rs` against a fake Kubernetes API: a mock server that serves the objects the
//! executor creates and the pod, log and Job status it then reads, with a kubeconfig that points at it.

use crate::env_support::kubernetes_env as env_lock;
use conveyor_service::domain::Status;
use conveyor_service::executors::engine::{
    ExecError, Handle, JobExecutor, JobSpec, JobState, SourceSpec,
};
use conveyor_service::executors::kubernetes::KubernetesExecutor;
use conveyor_service::executors::manifest::{self, FILE_MARKER, STEP_MARKER};
use conveyor_service::pipeline::{Readiness, Service, Step};
use conveyor_service::secrets::Redactor;
use conveyor_service::workspace::Workspace;
use quench_test::mock::matchers::{method, path, query_param};
use quench_test::mock::{Mock, MockServer, ResponseTemplate};
use std::collections::BTreeMap;
use std::time::Duration;

const NAMESPACE: &str = "ci";
const JOB_ID: &str = "0f8f0e0e-1111-2222-3333-444455556666";

fn json(status: u16, body: &quench_serde::Value) -> ResponseTemplate {
    ResponseTemplate::new(status)
        .set_body_raw(quench_serde::json::to_string(body), "application/json")
}

async fn serve(server: &MockServer, verb: &str, route: &str, response: ResponseTemplate) {
    Mock::given(method(verb))
        .and(path(route))
        .respond_with(response)
        .mount(server)
        .await;
}

fn executor_for(server: &MockServer, dir: &quench_core::tmp::TempDir) -> KubernetesExecutor {
    executor_with(server, dir, &[])
}

/// An executor connected to the fake cluster, with extra settings in the environment for as long as it
/// takes to read them.
fn executor_with(
    server: &MockServer,
    dir: &quench_core::tmp::TempDir,
    settings: &[(&str, &str)],
) -> KubernetesExecutor {
    let kubeconfig = dir.path().join("kubeconfig");
    std::fs::write(
        &kubeconfig,
        format!(
            "apiVersion: v1
kind: Config
current-context: fake
contexts:
  - name: fake
    context: {{cluster: fake, user: fake}}
clusters:
  - name: fake
    cluster: {{server: '{}'}}
users:
  - name: fake
    user: {{token: t}}
",
            server.uri()
        ),
    )
    .unwrap();
    quench_core::env::set("KUBECONFIG", kubeconfig.display().to_string());
    quench_core::env::set("CONVEYOR_K8S_NAMESPACE", NAMESPACE);
    for (name, value) in settings {
        quench_core::env::set(name, *value);
    }
    let executor = KubernetesExecutor::connect().expect("an executor");
    for (name, _) in settings {
        quench_core::env::remove(name);
    }
    executor
}

fn spec(steps: Vec<Step>) -> JobSpec {
    JobSpec {
        id: JOB_ID.to_string(),
        name: "build/cargo".to_string(),
        steps,
        env: BTreeMap::from([("API_TOKEN".to_string(), "hunter2-secret".to_string())]),
        timeout: Duration::from_mins(5),
        image: Some("rust:1".to_string()),
        source: Some(SourceSpec {
            clone_url: "https://example.invalid/thing.git".to_string(),
            git_ref: "refs/heads/master".to_string(),
            sha: "a".repeat(40),
            credential: None,
        }),
        redactor: Redactor::new(["hunter2-secret"]),
        services: Vec::new(),
        exports: vec!["out.txt".to_string()],
    }
}

fn two_steps() -> Vec<Step> {
    vec![
        Step::Run("cargo build".into()),
        Step::Run("cargo test".into()),
    ]
}

fn paths() -> (String, String, String) {
    let name = manifest::object_name(JOB_ID);
    (
        format!("/apis/batch/v1/namespaces/{NAMESPACE}/jobs"),
        format!("/api/v1/namespaces/{NAMESPACE}/secrets"),
        name,
    )
}

/// The cluster as it is while a job runs: creation accepted, one running pod with a log to read.
async fn serve_running_pod(server: &MockServer, log: &str) {
    let (jobs, secrets, name) = paths();
    serve(
        server,
        "POST",
        &secrets,
        json(201, &quench_serde::json!({})),
    )
    .await;
    serve(server, "POST", &jobs, json(201, &quench_serde::json!({}))).await;
    serve(
        server,
        "GET",
        &format!("/api/v1/namespaces/{NAMESPACE}/pods"),
        json(
            200,
            &quench_serde::json!({"items": [
                {"metadata": {"name": format!("{name}-abc")}, "status": {"phase": "Running"}}
            ]}),
        ),
    )
    .await;
    serve(
        server,
        "GET",
        &format!("/api/v1/namespaces/{NAMESPACE}/pods/{name}-abc/log"),
        ResponseTemplate::new(200).set_body_raw(log.to_string(), "text/plain"),
    )
    .await;
}

async fn finished(executor: &KubernetesExecutor, handle: &Handle) -> JobState {
    for _ in 0..200 {
        let state = executor.poll(handle).await.expect("poll");
        if state.is_finished() {
            return state;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the job never finished");
}

fn workspace() -> (Workspace, quench_core::tmp::TempDir) {
    let dir = quench_core::tmp::TempDir::new().unwrap();
    (Workspace::new(dir.path().to_path_buf()), dir)
}

#[allow(clippy::await_holding_lock)] // KUBECONFIG is process-global; held for the whole test
#[quench_test::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_job_that_succeeds_reports_its_steps_log_and_files() {
    let _env = env_lock();
    let server = MockServer::start().await;
    let dir = quench_core::tmp::TempDir::new().unwrap();
    let log = format!(
        "{STEP_MARKER}0\ncompiling with hunter2-secret in the environment\n{STEP_MARKER}1\n{FILE_MARKER}out.txt\taGVsbG8=\nall tests passed\n"
    );
    let (jobs, _, name) = paths();
    serve_running_pod(&server, &log).await;
    serve(
        &server,
        "GET",
        &format!("{jobs}/{name}"),
        json(200, &quench_serde::json!({"status": {"succeeded": 1}})),
    )
    .await;
    let executor = executor_for(&server, &dir);
    assert_eq!(executor.name(), "kubernetes");
    let (workspace, ws_dir) = workspace();

    let handle = executor
        .start(&spec(two_steps()), &workspace)
        .await
        .unwrap();
    assert_eq!(handle.as_str(), JOB_ID);
    let state = finished(&executor, &handle).await;

    assert_eq!(state.status, Status::Success);
    assert_eq!(state.exit_code, Some(0));
    assert_eq!(state.error, None);
    assert_eq!(state.steps.len(), 2);
    assert!(
        state.steps.iter().all(|s| s.status == Status::Success),
        "{:?}",
        state.steps
    );
    assert_eq!(state.steps[0].command, "cargo build");

    // the log has the build output - redacted - but neither bookkeeping marker
    let tail = executor.logs(&handle).await.unwrap();
    let lines: Vec<&str> = tail.history.iter().map(|c| c.line.as_str()).collect();
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert!(
        lines[0].starts_with("compiling with ") && !lines[0].contains("hunter2-secret"),
        "{lines:?}"
    );
    assert_eq!(lines[1], "all tests passed");
    assert_eq!(tail.history[1].seq, 1);

    // the exported file reached conveyor's workspace
    assert_eq!(
        std::fs::read_to_string(ws_dir.path().join("out.txt")).unwrap(),
        "hello"
    );

    // what was created: the secret first, then the Job, which names it
    let sent = server.received_requests().await.unwrap();
    let posts: Vec<&str> = sent
        .iter()
        .filter(|r| r.method == "POST")
        .map(|r| r.path.rsplit('/').next().unwrap())
        .collect();
    assert_eq!(posts, ["secrets", "jobs"]);
}

#[allow(clippy::await_holding_lock)]
#[quench_test::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failed_job_fails_the_step_that_was_running_and_skips_the_rest() {
    let _env = env_lock();
    let server = MockServer::start().await;
    let dir = quench_core::tmp::TempDir::new().unwrap();
    let (jobs, _, name) = paths();
    serve_running_pod(
        &server,
        &format!("{STEP_MARKER}0\nerror: could not compile\n"),
    )
    .await;
    serve(
        &server,
        "GET",
        &format!("{jobs}/{name}"),
        json(
            200,
            &quench_serde::json!({"status": {"failed": 1, "conditions": [
                {"type": "Complete", "status": "False"},
                {"type": "Failed", "status": "True", "message": "Job has reached the specified backoff limit"}
            ]}}),
        ),
    )
    .await;
    let executor = executor_for(&server, &dir);
    let (workspace, _ws) = workspace();

    let handle = executor
        .start(&spec(two_steps()), &workspace)
        .await
        .unwrap();
    let state = finished(&executor, &handle).await;

    assert_eq!(state.status, Status::Failed);
    assert_eq!(state.exit_code, None);
    assert_eq!(
        state.error.as_deref(),
        Some("Job has reached the specified backoff limit")
    );
    assert_eq!(state.steps[0].status, Status::Failed);
    assert_eq!(state.steps[1].status, Status::Skipped);
}

#[allow(clippy::await_holding_lock)]
#[quench_test::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_job_deleted_underneath_it_reads_as_cancelled() {
    let _env = env_lock();
    let server = MockServer::start().await;
    let dir = quench_core::tmp::TempDir::new().unwrap();
    let (jobs, _, name) = paths();
    serve_running_pod(&server, "").await;
    serve(
        &server,
        "GET",
        &format!("{jobs}/{name}"),
        json(404, &quench_serde::json!({"reason": "NotFound"})),
    )
    .await;
    let executor = executor_for(&server, &dir);
    let (workspace, _ws) = workspace();

    let handle = executor
        .start(&spec(two_steps()), &workspace)
        .await
        .unwrap();
    let state = finished(&executor, &handle).await;
    assert_eq!(state.status, Status::Cancelled);
    assert_eq!(state.error.as_deref(), Some("the cluster job was deleted"));
}

#[allow(clippy::await_holding_lock)]
#[quench_test::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_job_is_refused_before_anything_reaches_the_cluster_when_it_cannot_run() {
    let _env = env_lock();
    let server = MockServer::start().await;
    let dir = quench_core::tmp::TempDir::new().unwrap();
    let executor = executor_for(&server, &dir);
    let (workspace, _ws) = workspace();

    let none = executor
        .start(&spec(Vec::new()), &workspace)
        .await
        .unwrap_err();
    assert!(matches!(none, ExecError::NoSteps { .. }), "{none}");

    let mut no_source = spec(two_steps());
    no_source.source = None;
    let error = executor.start(&no_source, &workspace).await.unwrap_err();
    assert!(error.to_string().contains("no source to fetch"), "{error}");

    // an unbalanced quote in a later step is found now, not mid-run (a `run` step is the shell's to judge)
    let bad_quote = spec(vec![
        Step::Run("echo ok".into()),
        Step::Anvil("test 'unterminated".into()),
    ]);
    assert!(executor.start(&bad_quote, &workspace).await.is_err());

    assert!(server.received_requests().await.unwrap().is_empty());

    // an unknown handle is an error, except for `forget`, which has nothing to do
    let unknown = Handle::new("nope");
    assert!(matches!(
        executor.poll(&unknown).await,
        Err(ExecError::UnknownHandle(_))
    ));
    assert!(executor.logs(&unknown).await.is_err());
    assert!(executor.cancel(&unknown).await.is_err());
    executor.forget(&unknown).await.unwrap();
}

#[allow(clippy::await_holding_lock)]
#[quench_test::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_privileged_service_is_refused_before_the_cluster_is_asked_unless_the_deployment_allows_it()
 {
    let _env = env_lock();
    let server = MockServer::start().await;
    let dir = quench_core::tmp::TempDir::new().unwrap();
    let (workspace, _ws) = workspace();
    let mut job = spec(two_steps());
    job.services = vec![Service {
        name: "dind".to_string(),
        image: "docker:dind".to_string(),
        env: BTreeMap::new(),
        args: Vec::new(),
        ready: Readiness::Command("docker info".to_string()),
        privileged: true,
    }];

    // Not allowed (the default): nothing reaches the cluster, and the reason says what to turn on.
    let executor = executor_for(&server, &dir);
    let error = executor
        .start(&job, &workspace)
        .await
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("privileged service(s) dind")
            && error.contains("CONVEYOR_K8S_ALLOW_PRIVILEGED"),
        "{error}"
    );
    assert!(server.received_requests().await.unwrap().is_empty());

    // Allowed: the privilege check passes (this fake cluster serves nothing, so the start fails later and for
    // another reason).
    let executor = executor_with(&server, &dir, &[("CONVEYOR_K8S_ALLOW_PRIVILEGED", "true")]);
    let error = executor
        .start(&job, &workspace)
        .await
        .unwrap_err()
        .to_string();
    assert!(!error.contains("privileged"), "{error}");
}

#[allow(clippy::await_holding_lock)]
#[quench_test::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_refusal_from_the_cluster_says_which_object_it_was() {
    let _env = env_lock();
    let server = MockServer::start().await;
    let dir = quench_core::tmp::TempDir::new().unwrap();
    let (jobs, secrets, _) = paths();
    let executor = executor_for(&server, &dir);
    let (workspace, _ws) = workspace();

    serve(
        &server,
        "POST",
        &secrets,
        json(403, &quench_serde::json!({"reason": "Forbidden"})),
    )
    .await;
    let error = executor
        .start(&spec(two_steps()), &workspace)
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("create the job's secret"),
        "{error}"
    );

    server.reset().await;
    serve(
        &server,
        "POST",
        &secrets,
        json(201, &quench_serde::json!({})),
    )
    .await;
    serve(
        &server,
        "POST",
        &jobs,
        json(422, &quench_serde::json!({"reason": "Invalid"})),
    )
    .await;
    let error = executor
        .start(&spec(two_steps()), &workspace)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("create the job"), "{error}");
}

#[allow(clippy::await_holding_lock)]
#[quench_test::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancel_deletes_the_job_and_forget_removes_what_the_job_left() {
    let _env = env_lock();
    let server = MockServer::start().await;
    let dir = quench_core::tmp::TempDir::new().unwrap();
    let (jobs, secrets, name) = paths();
    // a job that never ends: its pod is up, its log has begun, and the Job reports nothing yet
    serve_running_pod(&server, &format!("{STEP_MARKER}0\nworking\n")).await;
    serve(
        &server,
        "GET",
        &format!("{jobs}/{name}"),
        json(200, &quench_serde::json!({"status": {"active": 1}})),
    )
    .await;
    serve(
        &server,
        "DELETE",
        &format!("{jobs}/{name}"),
        json(200, &quench_serde::json!({})),
    )
    .await;
    serve(
        &server,
        "DELETE",
        &format!("{secrets}/{name}"),
        json(200, &quench_serde::json!({})),
    )
    .await;
    let executor = executor_for(&server, &dir);
    let (workspace, _ws) = workspace();

    let handle = executor
        .start(&spec(two_steps()), &workspace)
        .await
        .unwrap();
    // wait for the first step to be running
    for _ in 0..200 {
        if executor.poll(&handle).await.unwrap().steps[0].status == Status::Running {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }

    executor.cancel(&handle).await.unwrap();
    let state = executor.poll(&handle).await.unwrap();
    assert_eq!(state.status, Status::Cancelled);
    assert_eq!(state.error.as_deref(), Some("cancelled"));
    assert_eq!(state.steps[0].status, Status::Cancelled);
    assert_eq!(state.steps[1].status, Status::Skipped);
    // cancelling again changes nothing
    executor.cancel(&handle).await.unwrap();

    executor.forget(&handle).await.unwrap();
    assert!(matches!(
        executor.poll(&handle).await,
        Err(ExecError::UnknownHandle(_))
    ));
    let deletes: Vec<String> = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.method == "DELETE")
        .map(|r| r.path.clone())
        .collect();
    assert!(
        deletes
            .iter()
            .any(|p| p.ends_with(&format!("/jobs/{name}"))),
        "{deletes:?}"
    );
    assert!(
        deletes
            .iter()
            .any(|p| p.ends_with(&format!("/secrets/{name}"))),
        "{deletes:?}"
    );
}

#[allow(clippy::await_holding_lock)]
#[quench_test::test(flavor = "multi_thread", worker_threads = 2)]
async fn forget_tolerates_objects_that_are_already_gone_or_cannot_be_deleted() {
    let _env = env_lock();
    let server = MockServer::start().await;
    let dir = quench_core::tmp::TempDir::new().unwrap();
    let (jobs, secrets, name) = paths();
    serve_running_pod(&server, "").await;
    serve(
        &server,
        "GET",
        &format!("{jobs}/{name}"),
        json(200, &quench_serde::json!({"status": {"succeeded": 1}})),
    )
    .await;
    serve(
        &server,
        "DELETE",
        &format!("{jobs}/{name}"),
        json(404, &quench_serde::json!({"reason": "NotFound"})),
    )
    .await;
    serve(
        &server,
        "DELETE",
        &format!("{secrets}/{name}"),
        ResponseTemplate::new(500),
    )
    .await;
    let executor = executor_for(&server, &dir);
    let (workspace, _ws) = workspace();

    let handle = executor
        .start(&spec(two_steps()), &workspace)
        .await
        .unwrap();
    finished(&executor, &handle).await;
    // cancelling a finished job leaves its verdict alone
    executor.cancel(&handle).await.unwrap();
    assert_eq!(
        executor.poll(&handle).await.unwrap().status,
        Status::Success
    );
    // a 404 and a 500 are logged, never fatal
    executor.forget(&handle).await.unwrap();
}

#[allow(clippy::await_holding_lock)]
#[quench_test::test]
async fn connecting_needs_a_cluster() {
    let _env = env_lock();
    let dir = quench_core::tmp::TempDir::new().unwrap();
    quench_core::env::set(
        "KUBECONFIG",
        dir.path().join("missing").display().to_string(),
    );
    let home = std::env::var("HOME").ok();
    quench_core::env::set("HOME", dir.path().display().to_string());
    quench_core::env::remove("KUBERNETES_SERVICE_HOST");
    let result = KubernetesExecutor::connect();
    if let Some(home) = home {
        quench_core::env::set("HOME", home);
    }
    let Err(error) = result else {
        panic!("an executor without a cluster")
    };
    assert!(error.contains("could not reach the cluster"), "{error}");
}

// ---------------------------------------------------------------------------
// Services
// ---------------------------------------------------------------------------

fn postgres() -> Service {
    Service {
        name: "postgres".to_string(),
        image: "pgvector/pgvector:pg18".to_string(),
        env: BTreeMap::from([("POSTGRES_PASSWORD".to_string(), "postgres".to_string())]),
        args: Vec::new(),
        ready: Readiness::Command("pg_isready -U postgres".to_string()),
        privileged: false,
    }
}

fn with_postgres() -> JobSpec {
    JobSpec {
        services: vec![postgres()],
        ..spec(two_steps())
    }
}

fn pod_name() -> String {
    format!("{}-abc", paths().2)
}

/// A pod that never leaves `Pending`, with these container statuses.
async fn serve_pending_pod(server: &MockServer, init_statuses: &quench_serde::Value) {
    let (jobs, secrets, _) = paths();
    serve(
        server,
        "POST",
        &secrets,
        json(201, &quench_serde::json!({})),
    )
    .await;
    serve(server, "POST", &jobs, json(201, &quench_serde::json!({}))).await;
    serve(
        server,
        "GET",
        &format!("/api/v1/namespaces/{NAMESPACE}/pods"),
        json(
            200,
            &quench_serde::json!({"items": [{
                "metadata": {"name": pod_name()},
                "status": {"phase": "Pending", "initContainerStatuses": init_statuses},
            }]}),
        ),
    )
    .await;
}

/// What a service container logged, served for requests that ask for that container.
async fn serve_container_log(server: &MockServer, container: &str, log: &str) {
    Mock::given(method("GET"))
        .and(path(format!(
            "/api/v1/namespaces/{NAMESPACE}/pods/{}/log",
            pod_name()
        )))
        .and(query_param("container", container))
        .respond_with(ResponseTemplate::new(200).set_body_raw(log.to_string(), "text/plain"))
        .with_priority(1)
        .mount(server)
        .await;
}

#[allow(clippy::await_holding_lock)]
#[quench_test::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_job_with_a_service_creates_it_as_a_sidecar_and_runs_as_usual() {
    let _env = env_lock();
    let server = MockServer::start().await;
    let dir = quench_core::tmp::TempDir::new().unwrap();
    let (jobs, _, name) = paths();
    serve_running_pod(
        &server,
        &format!("{STEP_MARKER}0\ntests ran against the database\n"),
    )
    .await;
    serve(
        &server,
        "GET",
        &format!("{jobs}/{name}"),
        json(200, &quench_serde::json!({"status": {"succeeded": 1}})),
    )
    .await;
    let executor = executor_for(&server, &dir);
    let (workspace, _ws) = workspace();

    let handle = executor.start(&with_postgres(), &workspace).await.unwrap();
    let state = finished(&executor, &handle).await;
    assert_eq!(state.status, Status::Success);

    // the Job that was created carries the service as a sidecar after the checkout
    let sent = server.received_requests().await.unwrap();
    let job: quench_serde::Value = sent
        .iter()
        .find(|r| r.method == "POST" && r.path == jobs)
        .map(|r| quench_serde::json::from_slice(&r.body).unwrap())
        .expect("the Job was created");
    let init = job["spec"]["template"]["spec"]["initContainers"]
        .as_array()
        .unwrap();
    let names: Vec<_> = init.iter().map(|c| c["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["checkout", "postgres"]);
    assert_eq!(init[1]["restartPolicy"], "Always");
    assert_eq!(
        init[1]["startupProbe"]["exec"]["command"][2],
        "pg_isready -U postgres"
    );
}

#[allow(clippy::await_holding_lock)]
#[quench_test::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_service_whose_image_cannot_be_pulled_fails_the_job_and_says_so() {
    let _env = env_lock();
    let server = MockServer::start().await;
    let dir = quench_core::tmp::TempDir::new().unwrap();
    serve_pending_pod(
        &server,
        &quench_serde::json!([
            {"name": "checkout", "state": {"terminated": {"exitCode": 0}}},
            {"name": "postgres", "state": {"waiting": {
                "reason": "ImagePullBackOff",
                "message": "Back-off pulling image \"pgvector/pgvector:pg99\"",
            }}},
        ]),
    )
    .await;
    serve_container_log(
        &server,
        "postgres",
        "Error: manifest unknown\nsecond line\n",
    )
    .await;
    // one second for the pod and one for the services: the test does not wait minutes to be told
    let executor = executor_with(
        &server,
        &dir,
        &[
            ("CONVEYOR_K8S_POD_START_SECS", "1"),
            ("CONVEYOR_K8S_SERVICE_READY_SECS", "1"),
        ],
    );
    let (workspace, _ws) = workspace();

    let handle = executor.start(&with_postgres(), &workspace).await.unwrap();
    let state = finished(&executor, &handle).await;

    assert_eq!(state.status, Status::Failed);
    let error = state.error.expect("a reason");
    assert!(
        error.starts_with(
            "the pod did not start in time: postgres: ImagePullBackOff: Back-off pulling image"
        ),
        "{error}"
    );
    // and the service's own log is in the run's log, under a heading
    let tail = executor.logs(&handle).await.unwrap();
    let lines: Vec<&str> = tail.history.iter().map(|c| c.line.as_str()).collect();
    assert_eq!(
        lines,
        [
            "--- service postgres: last log lines ---",
            "[postgres] Error: manifest unknown",
            "[postgres] second line",
        ]
    );
    assert_eq!(tail.history[2].seq, 2, "the lines are numbered in order");
}

#[allow(clippy::await_holding_lock)]
#[quench_test::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_service_that_keeps_restarting_before_it_is_ready_is_named() {
    let _env = env_lock();
    let server = MockServer::start().await;
    let dir = quench_core::tmp::TempDir::new().unwrap();
    serve_pending_pod(
        &server,
        &quench_serde::json!([
            {"name": "postgres", "started": false, "restartCount": 3, "state": {"running": {}}},
        ]),
    )
    .await;
    // the service log cannot be read yet (the container is between restarts): the heading still appears
    Mock::given(method("GET"))
        .and(path(format!(
            "/api/v1/namespaces/{NAMESPACE}/pods/{}/log",
            pod_name()
        )))
        .respond_with(ResponseTemplate::new(400))
        .mount(&server)
        .await;
    let executor = executor_with(&server, &dir, &[("CONVEYOR_K8S_POD_START_SECS", "1")]);
    let (workspace, _ws) = workspace();

    let handle = executor
        .start(&spec(two_steps()), &workspace)
        .await
        .unwrap();
    let state = finished(&executor, &handle).await;
    assert_eq!(state.status, Status::Failed);
    assert_eq!(
        state.error.as_deref(),
        Some(
            "the pod did not start in time: postgres keeps restarting before it reports ready (restarted 3 times)"
        )
    );
}

#[allow(clippy::await_holding_lock)]
#[quench_test::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_pod_that_never_appears_or_is_pending_without_a_reason_fails_plainly() {
    let _env = env_lock();
    let server = MockServer::start().await;
    let dir = quench_core::tmp::TempDir::new().unwrap();
    let (jobs, secrets, name) = paths();
    serve(
        &server,
        "POST",
        &secrets,
        json(201, &quench_serde::json!({})),
    )
    .await;
    serve(&server, "POST", &jobs, json(201, &quench_serde::json!({}))).await;
    serve(
        &server,
        "GET",
        &format!("/api/v1/namespaces/{NAMESPACE}/pods"),
        json(200, &quench_serde::json!({"items": []})),
    )
    .await;
    let executor = executor_with(&server, &dir, &[("CONVEYOR_K8S_POD_START_SECS", "1")]);
    let (workspace, _ws) = workspace();

    let handle = executor
        .start(&spec(two_steps()), &workspace)
        .await
        .unwrap();
    let state = finished(&executor, &handle).await;
    assert_eq!(
        state.error,
        Some(format!("no pod was scheduled for {name}"))
    );

    // a pod that exists but is pending with nothing to report
    server.reset().await;
    serve_pending_pod(&server, &quench_serde::json!([])).await;
    let executor = executor_with(&server, &dir, &[("CONVEYOR_K8S_POD_START_SECS", "1")]);
    let mut job = spec(two_steps());
    job.id = "another-job".to_string();
    let handle = executor.start(&job, &workspace).await.unwrap();
    let state = finished(&executor, &handle).await;
    assert_eq!(
        state.error.as_deref(),
        Some("the pod did not start in time")
    );
}

#[allow(clippy::await_holding_lock)]
#[quench_test::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failed_job_shows_what_its_services_logged() {
    let _env = env_lock();
    let server = MockServer::start().await;
    let dir = quench_core::tmp::TempDir::new().unwrap();
    let (jobs, _, name) = paths();
    serve_running_pod(&server, &format!("{STEP_MARKER}0\nconnection refused\n")).await;
    serve_container_log(
        &server,
        "postgres",
        "FATAL: password authentication failed\n",
    )
    .await;
    serve(
        &server,
        "GET",
        &format!("{jobs}/{name}"),
        json(
            200,
            &quench_serde::json!({"status": {"failed": 1, "conditions": [
                {"type": "Failed", "status": "True", "message": "backoff limit"}
            ]}}),
        ),
    )
    .await;
    let executor = executor_for(&server, &dir);
    let (workspace, _ws) = workspace();

    let handle = executor.start(&with_postgres(), &workspace).await.unwrap();
    let state = finished(&executor, &handle).await;
    assert_eq!(state.status, Status::Failed);

    let tail = executor.logs(&handle).await.unwrap();
    let lines: Vec<&str> = tail.history.iter().map(|c| c.line.as_str()).collect();
    assert_eq!(lines[0], "connection refused");
    assert_eq!(lines[1], "--- service postgres: last log lines ---");
    assert_eq!(lines[2], "[postgres] FATAL: password authentication failed");
}

#[allow(clippy::await_holding_lock)]
#[quench_test::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_job_that_succeeds_does_not_dump_its_services_log() {
    let _env = env_lock();
    let server = MockServer::start().await;
    let dir = quench_core::tmp::TempDir::new().unwrap();
    let (jobs, _, name) = paths();
    serve_running_pod(&server, &format!("{STEP_MARKER}0\nok\n")).await;
    serve_container_log(&server, "postgres", "database system is ready\n").await;
    serve(
        &server,
        "GET",
        &format!("{jobs}/{name}"),
        json(200, &quench_serde::json!({"status": {"succeeded": 1}})),
    )
    .await;
    let executor = executor_for(&server, &dir);
    let (workspace, _ws) = workspace();

    let handle = executor.start(&with_postgres(), &workspace).await.unwrap();
    finished(&executor, &handle).await;
    let tail = executor.logs(&handle).await.unwrap();
    let lines: Vec<&str> = tail.history.iter().map(|c| c.line.as_str()).collect();
    assert_eq!(lines, ["ok"]);
}
