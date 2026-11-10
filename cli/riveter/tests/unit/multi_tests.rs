//! `--env a,b,c`: parsing the list, the checks made before anything is applied, and the order and
//! stopping behaviour of a run across several clusters.

use crate::env_support::cwd_lock;
use crate::fake_cluster::{FakeCluster, with_clusters};
use riveter::env::{ENV_VAR, Workspace, resolve_env, resolve_envs, with_workspace};
use riveter::multi::{EnvReport, Outcome, apply_many, diff_many, finish, preflight, run_in_order};
use riveter::render::{ResourceScope, Selector};
use riveter::repl::WaitPolicy;
use std::collections::HashMap;
use std::fs;

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(ToString::to_string).collect()
}

fn overlay(context: Option<&str>) -> String {
    let context = context.map_or_else(String::new, |c| format!("kube_context: {c}\n"));
    format!(
        "{context}env: demo\nnamespace_name: demo\nresources:\n  - kind: deployment\n    name: api\n    image: nginx\n"
    )
}

/// A temp dir holding `overlays/<env>/overlay.yaml` for each `(env, kube_context)`.
fn workspace(envs: &[(&str, Option<&str>)]) -> (quench_core::tmp::TempDir, Workspace) {
    let dir = quench_core::tmp::TempDir::new().unwrap();
    let overlays = dir.path().join("overlays");
    for (env, context) in envs {
        fs::create_dir_all(overlays.join(env)).unwrap();
        fs::write(overlays.join(env).join("overlay.yaml"), overlay(*context)).unwrap();
    }
    let workspace = Workspace {
        overlays_dir: Some(overlays),
        output_dir: Some(dir.path().join("manifests")),
        env_vars: Some(HashMap::new()),
        ..Workspace::default()
    };
    (dir, workspace)
}

// --- parsing the list ------------------------------------------------------

/// `resolve_env(s)` read `overlays/` relative to the cwd, so these hold the shared cwd lock.
fn in_cwd_with_overlays<T>(envs: &[&str], body: impl FnOnce() -> T) -> T {
    let _guard = cwd_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let dir = quench_core::tmp::TempDir::new().unwrap();
    for env in envs {
        let path = dir.path().join("overlays").join(env);
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("overlay.yaml"), "# overlay\n").unwrap();
    }
    let original = std::env::current_dir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();
    quench_core::env::remove(ENV_VAR);
    let result = body();
    std::env::set_current_dir(original).unwrap();
    result
}

#[quench_test::test]
fn a_list_keeps_the_order_it_was_given_in() {
    in_cwd_with_overlays(&["a", "b", "c"], || {
        assert_eq!(
            resolve_envs(Some("c, a ,b")).unwrap(),
            ["c", "a", "b"],
            "names are trimmed, and the rollout order is the order typed"
        );
    });
}

#[quench_test::test]
fn a_single_name_is_a_list_of_one() {
    in_cwd_with_overlays(&["a"], || {
        assert_eq!(resolve_envs(Some("a")).unwrap(), ["a"]);
    });
}

#[quench_test::test]
fn a_name_listed_twice_is_refused() {
    in_cwd_with_overlays(&["a", "b"], || {
        let err = resolve_envs(Some("a,b,a")).unwrap_err().to_string();
        assert!(err.contains("`a` is listed twice"), "{err}");
    });
}

#[quench_test::test]
fn an_empty_entry_is_refused() {
    in_cwd_with_overlays(&["a", "b"], || {
        for bad in ["a,,b", "a,", ",a", " , "] {
            let err = resolve_envs(Some(bad)).unwrap_err().to_string();
            assert!(err.contains("empty entry"), "{bad:?}: {err}");
        }
    });
}

#[quench_test::test]
fn one_missing_overlay_refuses_the_whole_list() {
    in_cwd_with_overlays(&["a"], || {
        let err = resolve_envs(Some("a,nope")).unwrap_err().to_string();
        assert!(err.contains("overlay not found"), "{err}");
    });
}

#[quench_test::test]
fn commands_that_take_one_environment_say_so_when_given_a_list() {
    in_cwd_with_overlays(&["a", "b"], || {
        let err = resolve_env(Some("a,b")).unwrap_err().to_string();
        assert!(err.contains("takes one environment"), "{err}");
        assert!(err.contains("`apply` and `diff`"), "{err}");
    });
}

// --- before anything is applied -------------------------------------------

#[quench_test::test]
fn preflight_returns_each_environments_context() {
    let (_dir, ws) = workspace(&[("a", Some("ctx-a")), ("b", Some("ctx-b"))]);
    let contexts = with_workspace(ws, || {
        preflight(
            &names(&["a", "b"]),
            ResourceScope::All,
            &Selector::default(),
        )
    })
    .unwrap();
    assert_eq!(contexts, ["ctx-a", "ctx-b"]);
}

#[quench_test::test]
fn preflight_refuses_an_environment_that_pins_no_context_and_names_it() {
    let (_dir, ws) = workspace(&[("a", Some("ctx-a")), ("b", None)]);
    let err = with_workspace(ws, || {
        preflight(
            &names(&["a", "b"]),
            ResourceScope::All,
            &Selector::default(),
        )
    })
    .unwrap_err()
    .to_string();
    assert!(err.contains("nothing was applied"), "{err}");
    assert!(err.contains("b: pins no kube_context"), "{err}");
    assert!(
        !err.contains("a: pins"),
        "only the offender is named: {err}"
    );
}

#[quench_test::test]
fn preflight_reports_every_problem_not_just_the_first() {
    let (_dir, ws) = workspace(&[("a", None), ("b", None)]);
    let err = with_workspace(ws, || {
        preflight(
            &names(&["a", "b"]),
            ResourceScope::All,
            &Selector::default(),
        )
    })
    .unwrap_err()
    .to_string();
    assert!(err.contains("a: pins") && err.contains("b: pins"), "{err}");
}

// --- sequencing ------------------------------------------------------------

const fn applied(count: usize) -> Outcome {
    Outcome::Applied {
        count,
        dry_run: false,
    }
}

#[quench_test::test]
fn environments_run_in_order_and_each_gets_a_row() {
    let mut seen = Vec::new();
    let reports = run_in_order(
        &names(&["a", "b", "c"]),
        &names(&["ca", "cb", "cc"]),
        |env| {
            seen.push(env.to_string());
            Ok(applied(1))
        },
    );
    assert_eq!(seen, ["a", "b", "c"]);
    assert_eq!(
        reports
            .iter()
            .map(|r| r.context.as_str())
            .collect::<Vec<_>>(),
        ["ca", "cb", "cc"]
    );
    assert!(reports.iter().all(|r| !r.outcome.is_failure()));
}

#[quench_test::test]
fn a_failure_stops_the_run_and_the_rest_are_not_attempted() {
    let mut seen = Vec::new();
    let reports = run_in_order(
        &names(&["a", "b", "c"]),
        &names(&["ca", "cb", "cc"]),
        |env| {
            seen.push(env.to_string());
            if env == "b" {
                quench_core::bail!("apply failed")
            }
            Ok(applied(1))
        },
    );
    assert_eq!(seen, ["a", "b"], "c was never started");
    assert_eq!(reports[0].outcome, applied(1));
    assert!(matches!(&reports[1].outcome, Outcome::Failed(why) if why.contains("apply failed")));
    assert_eq!(reports[2].outcome, Outcome::NotAttempted);
}

#[quench_test::test]
fn finish_succeeds_when_nothing_failed() {
    let reports = vec![EnvReport {
        env: "a".into(),
        context: "ca".into(),
        outcome: applied(2),
    }];
    assert!(finish(&reports).is_ok());
}

#[quench_test::test]
fn finish_fails_naming_the_environment_and_how_many_were_skipped() {
    let reports = vec![
        EnvReport {
            env: "a".into(),
            context: "ca".into(),
            outcome: Outcome::Failed("boom".into()),
        },
        EnvReport {
            env: "b".into(),
            context: "cb".into(),
            outcome: Outcome::NotAttempted,
        },
    ];
    let err = finish(&reports).unwrap_err().to_string();
    assert!(err.contains("1 environment(s) not attempted"), "{err}");
    assert!(err.contains("a: boom"), "{err}");
}

#[quench_test::test]
fn a_diff_that_differs_is_not_a_failure() {
    let reports = vec![EnvReport {
        env: "a".into(),
        context: "ca".into(),
        outcome: Outcome::Differs,
    }];
    assert!(finish(&reports).is_ok());
}

// --- against fake clusters ---------------------------------------------------

/// A cluster that has the namespace the overlays use.
fn cluster() -> FakeCluster {
    let cluster = FakeCluster::start();
    cluster.insert(
        "namespaces",
        r#"{"apiVersion":"v1","kind":"Namespace","metadata":{"name":"demo"}}"#,
    );
    cluster
}

const fn policy() -> WaitPolicy {
    WaitPolicy {
        enabled: false,
        timeout_seconds: 1,
    }
}

#[quench_test::test]
fn apply_goes_to_each_environments_own_cluster_in_order() {
    let (_dir, ws) = workspace(&[("canary", Some("ctx-canary")), ("prod", Some("ctx-prod"))]);
    let (canary, prod) = (cluster(), cluster());
    with_clusters(&[("ctx-canary", &canary), ("ctx-prod", &prod)], || {
        with_workspace(ws, || {
            apply_many(
                &names(&["canary", "prod"]),
                false,
                ResourceScope::All,
                &Selector::default(),
                policy(),
            )
        })
        .unwrap();
    });
    let (first, second) = (canary.first_asked().unwrap(), prod.first_asked().unwrap());
    assert!(first < second, "canary first: {first} {second}");
    assert!(!canary.calls().is_empty() && !prod.calls().is_empty());
}

#[quench_test::test]
fn a_failing_cluster_stops_the_rollout_before_the_next_one_is_touched() {
    let (_dir, ws) = workspace(&[
        ("canary", Some("ctx-canary")),
        ("bad", Some("ctx-bad")),
        ("prod", Some("ctx-prod")),
    ]);
    let (canary, bad, prod) = (cluster(), cluster(), cluster());
    // Whatever is applied to `bad` is refused.
    bad.refuses_everything();
    with_clusters(
        &[
            ("ctx-canary", &canary),
            ("ctx-bad", &bad),
            ("ctx-prod", &prod),
        ],
        || {
            let err = with_workspace(ws, || {
                apply_many(
                    &names(&["canary", "bad", "prod"]),
                    false,
                    ResourceScope::All,
                    &Selector::default(),
                    policy(),
                )
            })
            .unwrap_err()
            .to_string();

            assert!(err.contains("1 environment(s) not attempted"), "{err}");
            assert!(err.contains("bad:"), "{err}");
        },
    );
    assert!(canary.first_asked().is_some());
    assert!(bad.first_asked().is_some());
    assert_eq!(
        prod.first_asked(),
        None,
        "prod's cluster was never contacted"
    );
}

#[quench_test::test]
fn an_environment_that_pins_no_context_stops_everything_before_any_apply() {
    let (_dir, ws) = workspace(&[("canary", Some("ctx-canary")), ("prod", None)]);
    let canary = cluster();
    with_clusters(&[("ctx-canary", &canary)], || {
        let err = with_workspace(ws, || {
            apply_many(
                &names(&["canary", "prod"]),
                false,
                ResourceScope::All,
                &Selector::default(),
                policy(),
            )
        })
        .unwrap_err()
        .to_string();
        assert!(err.contains("prod: pins no kube_context"), "{err}");
    });
    assert_eq!(canary.first_asked(), None, "canary was not applied either");
}

#[quench_test::test]
fn diff_visits_each_cluster_in_order() {
    let (_dir, ws) = workspace(&[("a", Some("ctx-a")), ("b", Some("ctx-b"))]);
    let (a, b) = (cluster(), cluster());
    with_clusters(&[("ctx-a", &a), ("ctx-b", &b)], || {
        with_workspace(ws, || {
            diff_many(
                &names(&["a", "b"]),
                ResourceScope::All,
                &Selector::default(),
            )
        })
        .unwrap();
    });
    assert!(a.first_asked().unwrap() < b.first_asked().unwrap());
}
