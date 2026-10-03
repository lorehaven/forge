//! Dependency ordering and `wait:` gates: what the overlay language promises, and the sequence of
//! kubectl calls an apply makes to keep it.

use crate::env_support::cwd_lock;
use riveter::env::{Workspace, with_workspace};
use riveter::order::{WaitFor, gate_of, gates_of, order_by_dependencies};
use riveter::render::{
    ResourceRef, ResourceScope, Selector, generate_manifests_selected, render_to_string,
};
use riveter::repl::{JobState, WaitPolicy, kubectl_apply, parse_job_state};
use serde_yaml::Value as YamlValue;
use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

fn yaml(text: &str) -> YamlValue {
    serde_yaml::from_str(text).unwrap()
}

/// `kind/name` of each resource, in order.
fn order_of(data: &YamlValue) -> Vec<String> {
    data["resources"]
        .as_sequence()
        .unwrap()
        .iter()
        .map(|r| {
            let kind = r["kind"].as_str().unwrap();
            let name = r["name"].as_str().unwrap_or("-");
            format!("{kind}/{name}")
        })
        .collect()
}

fn ordered(text: &str) -> anyhow::Result<Vec<String>> {
    let mut data = yaml(text);
    order_by_dependencies(&mut data)?;
    Ok(order_of(&data))
}

fn refs(kind: &str, name: &str) -> ResourceRef {
    ResourceRef {
        kind: kind.to_string(),
        name: name.to_string(),
        immutable: false,
    }
}

// --- depends_on ------------------------------------------------------------

#[test]
fn an_overlay_without_depends_on_is_not_reordered() {
    let text = "resources:\n  - {kind: service, name: b}\n  - {kind: deployment, name: a}\n  - {kind: ingress, name: c}\n";
    assert_eq!(
        ordered(text).unwrap(),
        ["service/b", "deployment/a", "ingress/c"]
    );
}

#[test]
fn a_resource_moves_after_what_it_depends_on_and_nothing_else_moves() {
    let text = "resources:
  - {kind: deployment, name: web, depends_on: [job/migrate]}
  - {kind: service, name: web}
  - {kind: job, name: migrate}
  - {kind: ingress, name: web}
";
    // `web` waits for `migrate`; `service` and `ingress` keep their places relative to what is free to go.
    assert_eq!(
        ordered(text).unwrap(),
        [
            "service/web",
            "job/migrate",
            "deployment/web",
            "ingress/web"
        ]
    );
}

#[test]
fn ties_are_broken_by_the_order_the_overlay_is_written_in() {
    let text = "resources:
  - {kind: deployment, name: c, depends_on: [deployment/a]}
  - {kind: deployment, name: b, depends_on: [deployment/a]}
  - {kind: deployment, name: a}
";
    assert_eq!(
        ordered(text).unwrap(),
        ["deployment/a", "deployment/c", "deployment/b"]
    );
}

#[test]
fn chains_and_diamonds_resolve() {
    let text = "resources:
  - {kind: deployment, name: app, depends_on: [deployment/api, deployment/worker]}
  - {kind: deployment, name: api, depends_on: [deployment/db]}
  - {kind: deployment, name: worker, depends_on: [deployment/db]}
  - {kind: deployment, name: db}
";
    assert_eq!(
        ordered(text).unwrap(),
        [
            "deployment/db",
            "deployment/api",
            "deployment/worker",
            "deployment/app"
        ]
    );
}

#[test]
fn a_dependency_may_name_its_kind_in_any_case_or_plural() {
    let text = "resources:
  - {kind: Deployment, name: web, depends_on: [JOB/Migrate]}
  - {kind: job, name: migrate}
";
    assert_eq!(ordered(text).unwrap(), ["job/migrate", "Deployment/web"]);
    assert_eq!(
        ordered("resources:\n  - {kind: deployment, name: a, depends_on: [jobs/b]}\n  - {kind: job, name: b}\n").unwrap(),
        ["job/b", "deployment/a"]
    );
}

#[test]
fn an_unknown_dependency_names_both_resources() {
    let err = ordered("resources:\n  - {kind: deployment, name: web, depends_on: [job/nope]}\n")
        .unwrap_err();
    let message = err.to_string();
    assert!(message.contains("deployment/web"), "{message}");
    assert!(message.contains("job/nope"), "{message}");
    assert!(message.contains("does not declare"), "{message}");
}

#[test]
fn a_dependency_on_itself_is_refused() {
    let err = ordered("resources:\n  - {kind: job, name: x, depends_on: [job/x]}\n").unwrap_err();
    assert!(err.to_string().contains("depends_on itself"), "{err}");
}

#[test]
fn a_cycle_is_refused_and_every_resource_in_it_is_named() {
    let text = "resources:
  - {kind: deployment, name: a, depends_on: [deployment/b]}
  - {kind: deployment, name: b, depends_on: [deployment/c]}
  - {kind: deployment, name: c, depends_on: [deployment/a]}
  - {kind: service, name: unrelated}
";
    let message = ordered(text).unwrap_err().to_string();
    assert!(message.contains("cycle"), "{message}");
    for name in ["deployment/a", "deployment/b", "deployment/c"] {
        assert!(message.contains(name), "{message}");
    }
}

#[test]
fn malformed_depends_on_is_refused_with_the_resource_named() {
    for bad in [
        "depends_on: job/x",
        "depends_on: [x]",
        "depends_on: [\"/x\"]",
        "depends_on: [\"job/\"]",
        "depends_on: [1]",
    ] {
        let text =
            format!("resources:\n  - {{kind: job, name: x}}\n  - {{kind: job, name: y, {bad}}}\n");
        let err = ordered(&text).unwrap_err().to_string();
        assert!(err.contains("job/y"), "{bad}: {err}");
    }
}

#[test]
fn dependencies_are_ordered_after_variables_are_expanded() {
    // The foundry Job's name carries the release, which is a variable - and other resources name it.
    let overlay = "env: demo
namespace_name: demo
resources:
  - kind: deployment
    name: api
    image: nginx
    depends_on: [\"job/${MIGRATION}\"]
  - kind: job
    name: ${MIGRATION}
    image: busybox
";
    let vars: HashMap<String, String> = [("MIGRATION".to_string(), "migrate-7".to_string())].into();
    let manifest = render_to_string("demo", overlay, &vars).unwrap();
    let job = manifest.find("name: migrate-7").unwrap();
    let deployment = manifest.find("name: api").unwrap();
    assert!(
        job < deployment,
        "the job must be rendered first:\n{manifest}"
    );
}

// --- wait ------------------------------------------------------------------

fn gate(text: &str, kind: &str) -> anyhow::Result<Option<(WaitFor, Option<u64>)>> {
    gate_of(&yaml(text), &refs(kind, "x"))
}

#[test]
fn a_job_can_wait_for_completion_and_a_workload_for_readiness() {
    assert_eq!(
        gate("{wait: complete}", "job").unwrap(),
        Some((WaitFor::Complete, None))
    );
    for kind in [
        "deployment",
        "statefulset",
        "daemonset",
        "StatefulSet",
        "deployments",
    ] {
        assert_eq!(
            gate("{wait: ready}", kind).unwrap(),
            Some((WaitFor::Ready, None)),
            "{kind}"
        );
    }
}

#[test]
fn a_gate_may_set_its_own_timeout() {
    assert_eq!(
        gate("{wait: complete, wait_timeout: 900}", "job").unwrap(),
        Some((WaitFor::Complete, Some(900)))
    );
}

#[test]
fn no_wait_means_no_gate() {
    assert_eq!(gate("{}", "job").unwrap(), None);
    assert_eq!(gate("{wait: null}", "job").unwrap(), None);
}

#[test]
fn a_wait_that_does_not_fit_its_kind_is_refused_with_the_right_one_suggested() {
    let message = gate("{wait: complete}", "deployment")
        .unwrap_err()
        .to_string();
    assert!(message.contains("is for a job"), "{message}");
    assert!(message.contains("`wait: ready`"), "{message}");

    let message = gate("{wait: ready}", "job").unwrap_err().to_string();
    assert!(message.contains("`wait: complete`"), "{message}");

    assert!(gate("{wait: ready}", "service").is_err());
    assert!(gate("{wait: complete}", "cronjob").is_err());
}

#[test]
fn bad_wait_values_are_refused() {
    assert!(
        gate("{wait: soon}", "job")
            .unwrap_err()
            .to_string()
            .contains("not `soon`")
    );
    assert!(gate("{wait: 3}", "job").is_err());
    assert!(gate("{wait: true}", "job").is_err());
    assert!(
        gate("{wait_timeout: 5}", "job")
            .unwrap_err()
            .to_string()
            .contains("needs a `wait:`")
    );
    for bad in ["0", "-3", "soon", "1.5"] {
        assert!(
            gate(&format!("{{wait: complete, wait_timeout: {bad}}}"), "job").is_err(),
            "{bad}"
        );
    }
}

#[test]
fn gates_are_collected_in_overlay_order() {
    let data = yaml(
        "resources:
  - {kind: deployment, name: db, wait: ready}
  - {kind: service, name: db}
  - {kind: job, name: migrate, wait: complete, wait_timeout: 60}
",
    );
    let gates = gates_of(&data).unwrap();
    assert_eq!(gates.len(), 2);
    assert_eq!(gates[0].0.to_string(), "deployment/db");
    assert_eq!((gates[1].1, gates[1].2), (WaitFor::Complete, Some(60)));
}

#[test]
fn job_state_is_read_from_its_conditions() {
    assert_eq!(parse_job_state("True||"), JobState::Complete);
    assert_eq!(parse_job_state("true||\n"), JobState::Complete);
    assert_eq!(parse_job_state("||"), JobState::Running);
    assert_eq!(parse_job_state(""), JobState::Running);
    assert_eq!(
        parse_job_state("|True|Job has reached the specified backoff limit"),
        JobState::Failed("Job has reached the specified backoff limit".to_string())
    );
    assert_eq!(parse_job_state("|True|"), JobState::Failed(String::new()));
    // A message may itself contain the separator.
    assert_eq!(
        parse_job_state("|True|exited | with 1"),
        JobState::Failed("exited | with 1".to_string())
    );
}

// --- phases and the apply --------------------------------------------------

const GATED: &str = "\
env: demo
namespace_name: demo
resources:
  - kind: namespace
    immutable: true
  - kind: deployment
    name: db
    image: postgres
    wait: ready
  - kind: job
    name: migrate
    image: busybox
    depends_on: [deployment/db]
    wait: complete
    wait_timeout: 60
  - kind: deployment
    name: api
    image: nginx
    depends_on: [job/migrate]
  - kind: service
    name: api
";

fn workspace(overlay: &str) -> (tempfile::TempDir, Workspace) {
    let dir = tempfile::tempdir().unwrap();
    let overlays = dir.path().join("overlays");
    fs::create_dir_all(overlays.join("demo")).unwrap();
    fs::write(overlays.join("demo/overlay.yaml"), overlay).unwrap();
    let workspace = Workspace {
        overlays_dir: Some(overlays),
        output_dir: Some(dir.path().join("manifests")),
        env_vars: Some(HashMap::new()),
        ..Workspace::default()
    };
    (dir, workspace)
}

#[test]
fn a_render_with_gates_is_split_into_a_manifest_per_phase() {
    let (_dir, ws) = workspace(GATED);
    let rendered = with_workspace(ws, || {
        generate_manifests_selected("demo", ResourceScope::All, &Selector::default())
    })
    .unwrap();

    assert_eq!(rendered.phases.len(), 3);
    let held: Vec<Vec<String>> = rendered
        .phases
        .iter()
        .map(|p| p.resources.iter().map(ToString::to_string).collect())
        .collect();
    assert_eq!(held[0], ["namespace/demo", "deployment/db"]);
    assert_eq!(held[1], ["job/migrate"]);
    assert_eq!(held[2], ["deployment/api", "service/api"]);

    assert_eq!(
        rendered.phases[0].gate.as_ref().unwrap().wait,
        WaitFor::Ready
    );
    assert_eq!(rendered.phases[1].gate.as_ref().unwrap().timeout, Some(60));
    assert!(
        rendered.phases[2].gate.is_none(),
        "the last phase has nothing to wait for"
    );

    // Each phase's manifest holds exactly its own resources; the full one is still written whole.
    let first = fs::read_to_string(&rendered.phases[0].path).unwrap();
    assert!(first.contains("name: db") && !first.contains("name: migrate"));
    let whole = fs::read_to_string(&rendered.path).unwrap();
    assert!(
        whole.contains("name: db")
            && whole.contains("name: migrate")
            && whole.contains("name: api")
    );
    assert_eq!(rendered.resource_count, 5);
}

#[test]
fn a_render_with_no_gates_has_no_phases() {
    let (_dir, ws) = workspace(
        "env: demo\nnamespace_name: demo\nresources:\n  - kind: deployment\n    name: a\n    image: nginx\n",
    );
    let rendered = with_workspace(ws, || {
        generate_manifests_selected("demo", ResourceScope::All, &Selector::default())
    })
    .unwrap();
    assert!(rendered.phases.is_empty());
}

#[test]
fn only_the_gates_of_selected_resources_split_a_render() {
    let (_dir, ws) = workspace(GATED);
    let rendered = with_workspace(ws, || {
        generate_manifests_selected(
            "demo",
            ResourceScope::All,
            &Selector::parse(&["service", "deployment/api"]).unwrap(),
        )
    })
    .unwrap();
    assert!(rendered.phases.is_empty(), "nothing selected is gated");
}

/// A `kubectl` that records every call and plays a cluster: namespaces exist, rollouts succeed, applies
/// succeed unless told otherwise, and `get job` answers with a fixed state.
struct Cluster {
    log: std::path::PathBuf,
    _dir: tempfile::TempDir,
}

fn with_cluster<T>(
    job_state: &str,
    fail_apply_of: Option<&str>,
    body: impl FnOnce(&Cluster) -> T,
) -> T {
    with_cluster_and_namespace("present", job_state, fail_apply_of, body)
}

/// `namespace`: `present`, `absent` (kubectl finds nothing) or `unreachable` (kubectl cannot ask).
fn with_cluster_and_namespace<T>(
    namespace: &str,
    job_state: &str,
    fail_apply_of: Option<&str>,
    body: impl FnOnce(&Cluster) -> T,
) -> T {
    let _guard = cwd_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("calls.log");
    let script = format!(
        "#!/bin/sh
echo \"$@\" >> '{log}'
case \"$1 $2\" in
  \"get namespace\") case '{namespace}' in present) echo namespace/demo ;; unreachable) echo 'unable to connect' >&2; exit 1 ;; esac ;;
  \"config current-context\") echo test ;;
  \"get job\") printf '%s' '{job_state}' ;;
  \"apply -f\") case \"$3\" in *{fail}*) echo 'error from the server' >&2; exit 1 ;; esac ;;
esac
exit 0
",
        log = log.display(),
        namespace = namespace,
        fail = fail_apply_of.unwrap_or("NEVER-MATCHES-ANY-PATH"),
    );
    let path = dir.path().join("kubectl");
    fs::write(&path, script).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();

    let original = std::env::var("PATH").unwrap_or_default();
    envmnt::set("PATH", format!("{}:{original}", dir.path().display()));
    let result = body(&Cluster { log, _dir: dir });
    envmnt::set("PATH", &original);
    result
}

impl Cluster {
    /// The kubectl calls made, one string each, with temp paths trimmed to their file names.
    fn calls(&self) -> Vec<String> {
        fs::read_to_string(&self.log)
            .unwrap_or_default()
            .lines()
            .map(|line| {
                line.split(' ')
                    .map(|word| {
                        Path::new(word)
                            .file_name()
                            .and_then(|n| n.to_str())
                            .filter(|_| word.starts_with('/'))
                            .unwrap_or(word)
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .filter(|line| !line.starts_with("config ") && !line.starts_with("get namespace"))
            .collect()
    }
}

fn apply(
    ws: Workspace,
    dry: bool,
    wait: WaitPolicy,
) -> anyhow::Result<riveter::render::RenderedManifest> {
    with_workspace(ws, || {
        kubectl_apply("demo", dry, ResourceScope::All, &Selector::default(), wait)
    })
}

#[test]
fn an_apply_stops_at_each_gate_before_the_next_phase_goes_out() {
    let (_dir, ws) = workspace(GATED);
    with_cluster("True||", None, |cluster| {
        apply(ws, false, WaitPolicy::default()).unwrap();

        let calls = cluster.calls();
        let position = |needle: &str| {
            calls
                .iter()
                .position(|c| c.contains(needle))
                .unwrap_or_else(|| panic!("{needle} not in {calls:#?}"))
        };

        let phase1 = position("apply -f demo-manifests.phase-1.yaml");
        let db_ready = position("rollout status -n demo deployment/db");
        let phase2 = position("apply -f demo-manifests.phase-2.yaml");
        let job_done = position("get job migrate");
        let phase3 = position("apply -f demo-manifests.phase-3.yaml");

        // apply, wait, apply, wait, apply - never two phases without the wait between them.
        assert!(phase1 < db_ready && db_ready < phase2, "{calls:#?}");
        assert!(phase2 < job_done && job_done < phase3, "{calls:#?}");
        // The whole manifest is not applied in one go as well.
        assert!(
            !calls
                .iter()
                .any(|c| c.ends_with("apply -f demo-manifests.yaml")),
            "{calls:#?}"
        );
    });
}

#[test]
fn a_job_that_fails_stops_the_apply_and_says_what_was_and_was_not_applied() {
    let (_dir, ws) = workspace(GATED);
    with_cluster(
        "|True|Job has reached the specified backoff limit",
        None,
        |cluster| {
            let err = apply(ws, false, WaitPolicy::default())
                .unwrap_err()
                .to_string();

            assert!(
                err.contains("job/migrate failed: Job has reached the specified backoff limit"),
                "{err}"
            );
            assert!(err.contains("kubectl logs job/migrate"), "{err}");
            assert!(
                err.contains("applied: namespace/demo, deployment/db, job/migrate"),
                "{err}"
            );
            assert!(
                err.contains("not applied: deployment/api, service/api"),
                "{err}"
            );

            let calls = cluster.calls();
            assert!(
                !calls.iter().any(|c| c.contains("phase-3")),
                "the phase after a failed gate must not be applied: {calls:#?}"
            );
        },
    );
}

#[test]
fn a_job_that_never_finishes_times_out_naming_the_knob() {
    let overlay = GATED.replace("wait_timeout: 60", "wait_timeout: 1");
    let (_dir, ws) = workspace(&overlay);
    with_cluster("||", None, |cluster| {
        let started = std::time::Instant::now();
        let err = apply(ws, false, WaitPolicy::default())
            .unwrap_err()
            .to_string();

        assert!(
            err.contains("job/migrate did not complete within 1s"),
            "{err}"
        );
        assert!(err.contains("wait_timeout"), "{err}");
        assert!(
            started.elapsed() < std::time::Duration::from_secs(10),
            "should time out promptly"
        );
        assert!(!cluster.calls().iter().any(|c| c.contains("phase-3")));
    });
}

#[test]
fn the_apply_timeout_is_the_default_for_a_gate_that_sets_none() {
    // The db gate has no wait_timeout of its own, so `--timeout` applies to it.
    let (_dir, ws) = workspace(GATED);
    with_cluster("True||", None, |cluster| {
        apply(
            ws,
            false,
            WaitPolicy {
                enabled: true,
                timeout_seconds: 42,
            },
        )
        .unwrap();
        assert!(
            cluster
                .calls()
                .iter()
                .any(|c| c == "rollout status -n demo deployment/db --timeout=42s"),
            "{:#?}",
            cluster.calls()
        );
    });
}

#[test]
fn a_failed_apply_in_a_phase_says_which_phase_and_that_it_may_be_partial() {
    let (_dir, ws) = workspace(GATED);
    with_cluster("True||", Some("phase-2"), |cluster| {
        let err = apply(ws, false, WaitPolicy::default())
            .unwrap_err()
            .to_string();

        assert!(err.contains("kubectl apply failed in phase 2/3"), "{err}");
        assert!(
            err.contains("applied: namespace/demo, deployment/db"),
            "{err}"
        );
        assert!(
            err.contains("failed, possibly partly applied: job/migrate"),
            "{err}"
        );
        assert!(
            err.contains("not applied: deployment/api, service/api"),
            "{err}"
        );
        assert!(!cluster.calls().iter().any(|c| c.contains("phase-3")));
    });
}

#[test]
fn no_wait_applies_the_whole_ordered_manifest_in_one_go() {
    let (_dir, ws) = workspace(GATED);
    with_cluster("||", None, |cluster| {
        apply(
            ws,
            false,
            WaitPolicy {
                enabled: false,
                timeout_seconds: 300,
            },
        )
        .unwrap();

        let calls = cluster.calls();
        assert_eq!(calls, ["apply -f demo-manifests.yaml"], "{calls:#?}");
    });
}

#[test]
fn a_dry_run_applies_the_whole_manifest_client_side_and_waits_for_nothing() {
    let (_dir, ws) = workspace(GATED);
    with_cluster("||", None, |cluster| {
        apply(ws, true, WaitPolicy::default()).unwrap();

        let calls = cluster.calls();
        assert_eq!(
            calls,
            ["apply --dry-run=client -f demo-manifests.yaml"],
            "{calls:#?}"
        );
    });
}

#[test]
fn an_overlay_without_gates_is_applied_in_one_go_as_before() {
    let (_dir, ws) = workspace(
        "env: demo\nnamespace_name: demo\nresources:\n  - kind: namespace\n    immutable: true\n  - kind: deployment\n    name: a\n    image: nginx\n",
    );
    with_cluster("||", None, |cluster| {
        apply(ws, false, WaitPolicy::default()).unwrap();

        let calls = cluster.calls();
        assert_eq!(calls[0], "apply -f demo-manifests.yaml", "{calls:#?}");
        assert!(
            calls.iter().any(|c| c.starts_with("rollout status")),
            "rollouts are still awaited: {calls:#?}"
        );
    });
}

// --- creating the namespace on install -------------------------------------------------------

use riveter::repl::create_namespace_if_missing;

const WITH_NAMESPACE: &str = "env: demo\nnamespace_name: demo\nresources:\n  - kind: namespace\n    immutable: true\n  - kind: deployment\n    name: a\n    image: nginx\n";

#[test]
fn a_missing_namespace_is_created_with_only_the_namespace() {
    let (_dir, ws) = workspace(WITH_NAMESPACE);
    with_cluster_and_namespace("absent", "||", None, |cluster| {
        assert!(with_workspace(ws, || create_namespace_if_missing("demo")).unwrap());

        let calls = cluster.calls();
        assert_eq!(
            calls,
            ["apply -f demo-manifests.selection.yaml"],
            "{calls:#?}"
        );
    });
}

#[test]
fn an_existing_namespace_is_left_alone() {
    let (_dir, ws) = workspace(WITH_NAMESPACE);
    with_cluster_and_namespace("present", "||", None, |cluster| {
        assert!(!with_workspace(ws, || create_namespace_if_missing("demo")).unwrap());
        assert!(cluster.calls().is_empty(), "{:#?}", cluster.calls());
    });
}

#[test]
fn an_unreachable_cluster_is_not_mistaken_for_a_missing_namespace() {
    let (_dir, ws) = workspace(WITH_NAMESPACE);
    with_cluster_and_namespace("unreachable", "||", None, |cluster| {
        assert!(!with_workspace(ws, || create_namespace_if_missing("demo")).unwrap());
        assert!(
            cluster.calls().is_empty(),
            "nothing is applied to a cluster that could not be asked"
        );
    });
}

#[test]
fn an_overlay_that_declares_no_namespace_is_left_for_the_apply_to_report_on() {
    let (_dir, ws) = workspace(
        "env: demo\nnamespace_name: demo\nresources:\n  - kind: deployment\n    name: a\n    image: nginx\n",
    );
    with_cluster_and_namespace("absent", "||", None, |cluster| {
        assert!(!with_workspace(ws, || create_namespace_if_missing("demo")).unwrap());
        assert!(cluster.calls().is_empty());
    });
}
