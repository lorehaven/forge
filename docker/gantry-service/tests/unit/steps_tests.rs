use gantry_service::domain::commands::{Context, package_file};
use gantry_service::domain::runner::{Exec, Outcome, parse_outcome, run};
use gantry_service::domain::steps::{Plan, Step};
use std::collections::BTreeMap;
use std::path::PathBuf;

fn ctx(dry_run: bool) -> Context {
    Context {
        values_dir: PathBuf::from("/values"),
        packages_dir: PathBuf::from("/work/packages"),
        allowed_namespaces: vec![],
        dry_run,
        source_dir: None,
    }
}

fn install(package: &str) -> Step {
    Step::Install {
        package: package.into(),
        version: "1.2.3+b1".into(),
        namespace: "forge".into(),
        sets: BTreeMap::from([("HOST".to_string(), "example.org".to_string())]),
        replicas: BTreeMap::from([("deployment/sage".to_string(), 0)]),
        values_secret: Some(format!("gantry-values-{package}")),
        timeout_secs: Some(120),
        targets: vec![],
        except: vec![],
        no_wait: false,
    }
}

fn plan(steps: Vec<Step>) -> Plan {
    Plan {
        steps,
        summary: vec![],
        ..Default::default()
    }
}

#[test]
fn the_plan_round_trips_through_the_json_a_job_carries() {
    let original = plan(vec![
        Step::Pull {
            package: "forge".into(),
            version: "1.2.3".into(),
        },
        install("forge"),
    ]);
    let json = serde_json::to_string(&original).unwrap();
    assert!(json.contains(r#""step":"pull""#), "{json}");
    assert_eq!(serde_json::from_str::<Plan>(&json).unwrap(), original);
}

#[test]
fn an_install_is_a_riveter_command_on_the_downloaded_file_with_its_values_mounted() {
    let commands = install("forge").commands(&ctx(false));
    assert_eq!(commands.len(), 1);
    let command = &commands[0];
    assert_eq!(command.program, "riveter");
    assert!(command.mutating);
    assert_eq!(
        command.args,
        [
            "install",
            "/work/packages/forge-1.2.3+b1.rivet",
            // Whole: a package's ServiceAccounts and Ingresses are part of an install.
            "--scope",
            "all",
            "--env-file",
            "/values/gantry-values-forge/env",
            "--inventory",
            "--set",
            "HOST=example.org",
            "--replicas",
            "deployment/sage=0",
            "--timeout",
            "120",
        ]
    );
    assert_eq!(
        package_file(&PathBuf::from("/p"), "a", "1.0.0"),
        PathBuf::from("/p/a-1.0.0.rivet")
    );
}

#[test]
fn a_check_is_a_dry_run_that_changes_nothing() {
    let step = Step::Check {
        package: "forge".into(),
        version: "1.0.0".into(),
        values_secret: None,
    };
    let command = &step.commands(&ctx(false))[0];
    assert!(!command.mutating);
    assert_eq!(command.args.last().unwrap(), "--dry-run");
    assert!(!command.args.contains(&"--env-file".to_string()));
}

#[test]
fn in_a_dry_run_an_install_is_riveters_own_dry_run_and_is_not_mutating() {
    let command = &install("forge").commands(&ctx(true))[0];
    assert!(!command.mutating);
    assert_eq!(command.args.last().unwrap(), "--dry-run");
}

#[test]
fn pod_steps_follow_the_allow_list_when_they_name_no_namespace() {
    let step = Step::DeletePods {
        selector: "app=vllm".into(),
        namespaces: vec![],
    };
    let mut context = ctx(false);
    assert_eq!(step.commands(&context)[0].args.last().unwrap(), "-A");

    context.allowed_namespaces = vec!["a".into(), "b".into()];
    let commands = step.commands(&context);
    assert_eq!(commands.len(), 2);
    assert_eq!(commands[1].args[commands[1].args.len() - 2..], ["-n", "b"]);
    assert!(commands.iter().all(|c| c.mutating));
}

#[test]
fn waiting_for_pods_to_go_is_a_listing_so_a_dry_run_really_waits() {
    let step = Step::WaitGone {
        selector: "app=vllm".into(),
        namespaces: vec!["ml".into()],
        timeout_secs: 90,
    };
    let command = &step.commands(&ctx(true))[0];
    assert!(!command.mutating);
    assert_eq!(command.args[..4], ["get", "pods", "-l", "app=vllm"]);
    assert_eq!(command.args[command.args.len() - 2..], ["-n", "ml"]);
}

fn wait_plan(timeout_secs: u64) -> Plan {
    plan(vec![Step::WaitGone {
        selector: "app=vllm".into(),
        namespaces: vec![],
        timeout_secs,
    }])
}

#[test]
fn a_wait_ends_as_soon_as_nothing_is_left_even_when_nothing_ever_matched() {
    let mut exec = Recorder::default();
    let outcome = run(&wait_plan(60), &ctx(false), &mut exec, &mut Vec::new());
    assert_eq!(outcome, Outcome::Succeeded);
    assert_eq!(exec.slept, 0);
}

#[test]
fn a_wait_polls_until_the_pods_are_gone() {
    let mut exec = Recorder {
        listings: ["pod/a\npod/b\n".to_string(), "pod/b\n".to_string()].into(),
        ..Default::default()
    };
    let mut out = Vec::new();
    let outcome = run(&wait_plan(60), &ctx(false), &mut exec, &mut out);
    assert_eq!(outcome, Outcome::Succeeded);
    assert_eq!(exec.slept, 2);
    assert!(
        String::from_utf8(out)
            .unwrap()
            .contains("waiting for 2 pod(s)")
    );
}

#[test]
fn a_wait_that_runs_out_of_time_names_what_is_still_there() {
    let mut exec = Recorder {
        listings: std::iter::repeat_n("pod/stuck\n".to_string(), 50).collect(),
        ..Default::default()
    };
    let outcome = run(&wait_plan(4), &ctx(false), &mut exec, &mut Vec::new());
    let Outcome::Failed(reason) = outcome else {
        panic!("expected a timeout")
    };
    assert!(reason.contains("pod/stuck"), "{reason}");
    assert!(reason.contains("4s"), "{reason}");
}

#[test]
fn only_a_rollout_that_asks_for_it_has_an_undo() {
    let guarded = Step::Rollout {
        namespace: "forge".into(),
        kind: "deployment".into(),
        name: "gantry".into(),
        timeout_secs: 60,
        rollback_on_failure: true,
    };
    let undo = guarded.rollback().expect("has an undo");
    assert_eq!(undo.args[2..], ["rollout", "undo", "deployment/gantry"]);

    let plain = Step::Rollout {
        namespace: "forge".into(),
        kind: "deployment".into(),
        name: "gantry".into(),
        timeout_secs: 60,
        rollback_on_failure: false,
    };
    assert!(plain.rollback().is_none());
}

#[test]
fn validation_refuses_anything_that_could_read_as_a_flag_or_a_second_argument() {
    let bad = |step: Step| plan(vec![step]).validate(&[]).unwrap_err();

    assert!(
        bad(Step::Pull {
            package: "--help".into(),
            version: "1.0.0".into()
        })
        .contains("package")
    );
    assert!(
        bad(Step::Pull {
            package: "forge".into(),
            version: "1.0; rm -rf /".into()
        })
        .contains("version")
    );
    assert!(
        bad(Step::DeletePods {
            selector: "-A".into(),
            namespaces: vec![]
        })
        .contains("selector")
    );
    assert!(
        bad(Step::Scale {
            namespace: "forge".into(),
            kind: "secret".into(),
            name: "x".into(),
            replicas: 0
        })
        .contains("workload kind")
    );
    assert!(
        bad(Step::Install {
            package: "forge".into(),
            version: "1.0.0".into(),
            namespace: "forge".into(),
            sets: BTreeMap::from([("--x".to_string(), "1".to_string())]),
            replicas: BTreeMap::new(),
            values_secret: None,
            timeout_secs: None,
            targets: vec![],
            except: vec![],
            no_wait: false,
        })
        .contains("variable")
    );
    assert!(plan(vec![]).validate(&[]).is_err());
}

#[test]
fn a_selector_cannot_smuggle_an_argument_in_through_a_space() {
    // A space is legal in `in (a, b)`, and harmless: each value is one argument, never split.
    let step = Step::DeletePods {
        selector: "tier in (a, b)".into(),
        namespaces: vec![],
    };
    assert!(plan(vec![step.clone()]).validate(&[]).is_ok());
    assert_eq!(step.commands(&ctx(false))[0].args[3], "tier in (a, b)");
}

#[test]
fn namespaces_outside_the_allow_list_are_refused() {
    let allowed = vec!["scratch".to_string()];
    let error = plan(vec![install("forge")]).validate(&allowed).unwrap_err();
    assert!(error.contains("allow-list"), "{error}");
    assert!(error.contains("scratch"), "{error}");
    assert!(plan(vec![install("forge")]).validate(&[]).is_ok());
}

#[test]
fn the_secrets_a_plan_reads_are_listed_once() {
    let both = plan(vec![install("forge"), install("forge"), install("media")]);
    assert_eq!(
        both.values_secrets(),
        ["gantry-values-forge", "gantry-values-media"]
    );
}

// ---------------------------------------------------------------- the runner loop

#[derive(Default)]
struct Recorder {
    ran: Vec<String>,
    /// Exit codes by the program+first-arg prefix a test wants to fail.
    fail_on: Vec<(String, i32)>,
    /// What successive listings answer; empty once exhausted.
    listings: std::collections::VecDeque<String>,
    slept: u32,
}

impl Exec for Recorder {
    fn capture(
        &mut self,
        command: &gantry_service::domain::commands::Command,
    ) -> std::io::Result<(i32, String)> {
        self.ran.push(command.display());
        Ok((0, self.listings.pop_front().unwrap_or_default()))
    }

    fn sleep(&mut self, _: std::time::Duration) {
        self.slept += 1;
    }

    fn run(&mut self, command: &gantry_service::domain::commands::Command) -> std::io::Result<i32> {
        let line = command.display();
        self.ran.push(line.clone());
        Ok(self
            .fail_on
            .iter()
            .find(|(needle, _)| line.contains(needle.as_str()))
            .map_or(0, |(_, code)| *code))
    }
}

fn pull_install_wait() -> Plan {
    plan(vec![
        Step::Pull {
            package: "forge".into(),
            version: "1.2.3".into(),
        },
        install("forge"),
        Step::Rollout {
            namespace: "forge".into(),
            kind: "deployment".into(),
            name: "gantry".into(),
            timeout_secs: 60,
            rollback_on_failure: true,
        },
    ])
}

#[test]
fn a_run_goes_step_by_step_and_ends_with_the_outcome_marker() {
    let mut exec = Recorder::default();
    let mut out = Vec::new();
    let outcome = run(&pull_install_wait(), &ctx(false), &mut exec, &mut out);

    assert_eq!(outcome, Outcome::Succeeded);
    assert_eq!(exec.ran.len(), 3);
    assert!(exec.ran[0].starts_with("riveter pull forge@1.2.3"));
    let text = String::from_utf8(out).unwrap();
    assert!(
        text.contains("== step 2/3: install forge 1.2.3+b1"),
        "{text}"
    );
    assert_eq!(parse_outcome(&text), Some(Outcome::Succeeded));
}

#[test]
fn the_first_failure_stops_the_run_and_nothing_after_it_is_attempted() {
    let mut exec = Recorder {
        fail_on: vec![("riveter install".into(), 1)],
        ..Default::default()
    };
    let mut out = Vec::new();
    let outcome = run(&pull_install_wait(), &ctx(false), &mut exec, &mut out);

    let Outcome::Failed(reason) = outcome else {
        panic!("expected a failure")
    };
    assert!(reason.contains("step 2"), "{reason}");
    assert_eq!(exec.ran.len(), 2, "the rollout step must not have run");
}

#[test]
fn a_rollout_that_fails_is_undone_and_reported_as_rolled_back() {
    let mut exec = Recorder {
        fail_on: vec![("rollout status".into(), 1)],
        ..Default::default()
    };
    let mut out = Vec::new();
    let outcome = run(&pull_install_wait(), &ctx(false), &mut exec, &mut out);

    assert!(matches!(outcome, Outcome::RolledBack(_)), "{outcome:?}");
    assert!(
        exec.ran
            .last()
            .unwrap()
            .contains("rollout undo deployment/gantry")
    );
    let text = String::from_utf8(out).unwrap();
    assert!(matches!(parse_outcome(&text), Some(Outcome::RolledBack(_))));
}

#[test]
fn a_failed_undo_is_a_failure_not_a_rollback() {
    let mut exec = Recorder {
        fail_on: vec![("rollout status".into(), 1), ("rollout undo".into(), 1)],
        ..Default::default()
    };
    let outcome = run(
        &pull_install_wait(),
        &ctx(false),
        &mut exec,
        &mut Vec::new(),
    );
    let Outcome::Failed(reason) = outcome else {
        panic!("expected a failure")
    };
    assert!(reason.contains("rollback also failed"), "{reason}");
}

#[test]
fn a_dry_run_prints_what_would_change_and_runs_only_what_cannot() {
    let scale = Step::Scale {
        namespace: "forge".into(),
        kind: "deployment".into(),
        name: "sage".into(),
        replicas: 0,
    };
    let wait = Step::Rollout {
        namespace: "forge".into(),
        kind: "deployment".into(),
        name: "sage".into(),
        timeout_secs: 5,
        rollback_on_failure: false,
    };
    let mut exec = Recorder::default();
    let mut out = Vec::new();
    run(&plan(vec![scale, wait]), &ctx(true), &mut exec, &mut out);

    assert_eq!(exec.ran.len(), 1, "{:?}", exec.ran);
    assert!(exec.ran[0].contains("rollout status"));
    let text = String::from_utf8(out).unwrap();
    assert!(text.contains("(dry run: not executed)"), "{text}");
    assert!(
        text.contains("$ kubectl -n forge scale deployment/sage --replicas=0"),
        "{text}"
    );
}

#[test]
fn the_runner_refuses_a_plan_outside_its_allow_list_before_touching_anything() {
    let mut context = ctx(false);
    context.allowed_namespaces = vec!["scratch".into()];
    let mut exec = Recorder::default();
    let outcome = run(
        &plan(vec![install("forge")]),
        &context,
        &mut exec,
        &mut Vec::new(),
    );
    assert!(matches!(outcome, Outcome::Failed(_)));
    assert!(exec.ran.is_empty());
}

#[test]
fn the_outcome_is_read_from_the_last_marker_in_a_log() {
    assert_eq!(parse_outcome("hello\n"), None);
    assert_eq!(
        parse_outcome("a\nGANTRY-RESULT: failed boom here\nGANTRY-RESULT: ok\n"),
        Some(Outcome::Succeeded)
    );
    assert_eq!(
        parse_outcome("GANTRY-RESULT: failed boom here"),
        Some(Outcome::Failed("boom here".into()))
    );
}

#[test]
fn with_a_source_directory_a_pull_is_a_copy() {
    let mut context = ctx(false);
    context.source_dir = Some(PathBuf::from("/local/packages"));
    let step = Step::Pull {
        package: "forge".into(),
        version: "1.2.3".into(),
    };
    let command = &step.commands(&context)[0];
    assert_eq!(command.program, "cp");
    assert_eq!(
        command.args,
        ["/local/packages/forge-1.2.3.rivet", "/work/packages"]
    );
    assert!(!command.mutating);
}

#[test]
fn an_install_can_leave_resources_out_and_return_without_waiting() {
    let step = Step::Install {
        package: "forge".into(),
        version: "1.0.0".into(),
        namespace: "forge".into(),
        sets: BTreeMap::new(),
        replicas: BTreeMap::new(),
        values_secret: None,
        timeout_secs: None,
        targets: vec!["deployment/gantry".into()],
        except: vec!["deployment/other".into()],
        no_wait: true,
    };
    let args = &step.commands(&ctx(false))[0].args;
    assert!(
        args.windows(2)
            .any(|w| w == ["--except", "deployment/other"]),
        "{args:?}"
    );
    assert!(args.contains(&"--no-wait".to_string()));
    assert_eq!(
        args.last().unwrap(),
        "deployment/gantry",
        "targets are positional, last"
    );
}
