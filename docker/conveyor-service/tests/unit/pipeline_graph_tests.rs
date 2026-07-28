//! Unit tests for `pipeline/graph.rs`: ordering, cycle detection, and what a
//! particular run decides to execute.

use conveyor_service::pipeline::graph::{Decision, GraphError, topological_order};
use conveyor_service::pipeline::spec::{Job, Stage, Step};
use conveyor_service::pipeline::{EvalContext, parse, plan};
use std::collections::BTreeMap;

fn job(name: &str) -> Job {
    Job {
        name: name.to_string(),
        when: None,
        env: BTreeMap::new(),
        secrets: Vec::new(),
        timeout: None,
        image: None,
        artifacts: Vec::new(),
        steps: vec![Step::Run("true".to_string())],
    }
}

fn stage(name: &str, needs: &[&str]) -> Stage {
    Stage {
        name: name.to_string(),
        needs: needs.iter().map(|n| (*n).to_string()).collect(),
        when: None,
        jobs: vec![job(name)],
    }
}

fn names(stages: &[Stage], order: &[usize]) -> Vec<String> {
    order.iter().map(|&i| stages[i].name.clone()).collect()
}

// ---------------------------------------------------------------------------
// Ordering
// ---------------------------------------------------------------------------

#[test]
fn independent_stages_keep_declaration_order() {
    let stages = vec![stage("a", &[]), stage("b", &[]), stage("c", &[])];
    let order = topological_order(&stages).expect("orders");
    assert_eq!(names(&stages, &order), ["a", "b", "c"]);
}

#[test]
fn a_stage_follows_what_it_needs() {
    // Declared backwards on purpose: the order has to come from `needs`, not
    // from where the author happened to put the tables.
    let stages = vec![
        stage("deploy", &["test"]),
        stage("test", &["build"]),
        stage("build", &[]),
    ];
    let order = topological_order(&stages).expect("orders");
    assert_eq!(names(&stages, &order), ["build", "test", "deploy"]);
}

#[test]
fn a_diamond_is_ordered_with_ties_in_declaration_order() {
    let stages = vec![
        stage("build", &[]),
        stage("lint", &["build"]),
        stage("test", &["build"]),
        stage("deploy", &["lint", "test"]),
    ];
    let order = topological_order(&stages).expect("orders");
    assert_eq!(names(&stages, &order), ["build", "lint", "test", "deploy"]);
}

#[test]
fn ordering_is_deterministic() {
    // A run report that reshuffles between runs is one nobody can compare
    // against the last one.
    let stages = vec![
        stage("c", &["a"]),
        stage("b", &["a"]),
        stage("a", &[]),
        stage("d", &["b", "c"]),
    ];
    let first = topological_order(&stages).expect("orders");
    for _ in 0..20 {
        assert_eq!(topological_order(&stages).expect("orders"), first);
    }

    // `a` first because both of the others need it, then `c` before `b`
    // because that is the order they appear in the file - not alphabetically,
    // which would reorder a pipeline behind its author's back.
    assert_eq!(names(&stages, &first), ["a", "c", "b", "d"]);
}

#[test]
fn a_stage_with_several_needs_follows_all_of_them() {
    let stages = vec![
        stage("deploy", &["build", "test"]),
        stage("build", &[]),
        stage("test", &[]),
    ];
    let order = topological_order(&stages).expect("orders");
    let ordered = names(&stages, &order);
    let position = |name: &str| ordered.iter().position(|s| s == name).expect("present");
    assert!(position("build") < position("deploy"));
    assert!(position("test") < position("deploy"));
}

// ---------------------------------------------------------------------------
// Rejected graphs
// ---------------------------------------------------------------------------

#[test]
fn duplicate_stage_names_are_rejected() {
    let stages = vec![stage("build", &[]), stage("build", &[])];
    assert_eq!(
        topological_order(&stages),
        Err(GraphError::DuplicateStage {
            name: "build".to_string()
        })
    );
}

#[test]
fn a_needs_naming_no_stage_is_rejected() {
    let stages = vec![stage("build", &[]), stage("deploy", &["tset"])];
    assert_eq!(
        topological_order(&stages),
        Err(GraphError::UnknownNeeds {
            stage: "deploy".to_string(),
            missing: "tset".to_string(),
        })
    );
}

#[test]
fn a_stage_needing_itself_is_rejected() {
    let stages = vec![stage("build", &["build"])];
    assert_eq!(
        topological_order(&stages),
        Err(GraphError::SelfNeeds {
            stage: "build".to_string()
        })
    );
}

#[test]
fn a_cycle_is_rejected_and_named() {
    let stages = vec![
        stage("a", &["c"]),
        stage("b", &["a"]),
        stage("c", &["b"]),
        stage("unrelated", &[]),
    ];
    let error = topological_order(&stages).expect_err("should not order");

    let GraphError::Cycle { path } = &error else {
        panic!("expected a cycle, got {error:?}");
    };
    // Naming the cycle beats "there is a cycle somewhere"; the author has to
    // find it either way.
    for name in ["a", "b", "c"] {
        assert!(
            path.iter().any(|s| s == name),
            "{path:?} should name {name}"
        );
    }
    assert!(
        !path.iter().any(|s| s == "unrelated"),
        "{path:?} should not drag in an unrelated stage"
    );
}

// ---------------------------------------------------------------------------
// Planning a run
// ---------------------------------------------------------------------------

const PIPELINE: &str = r#"
[[stage]]
name = "build"
[[stage.job]]
steps = ["cargo build"]

[[stage]]
name = "test"
needs = ["build"]
[[stage.job]]
steps = ["cargo test"]

[[stage]]
name = "deploy"
needs = ["test"]
when = "branch == 'master'"
[[stage.job]]
steps = ["deploy.sh"]
"#;

fn decisions(context: &EvalContext) -> Vec<(String, Decision)> {
    let spec = parse(PIPELINE).expect("parses");
    plan(&spec, context)
        .into_iter()
        .map(|stage_plan| {
            (
                spec.stages[stage_plan.index].name.clone(),
                stage_plan.decision,
            )
        })
        .collect()
}

#[test]
fn every_stage_runs_when_nothing_excludes_it() {
    let planned = decisions(&EvalContext::new("push", "refs/heads/master", "abc"));
    assert_eq!(
        planned,
        [
            ("build".to_string(), Decision::Run),
            ("test".to_string(), Decision::Run),
            ("deploy".to_string(), Decision::Run),
        ]
    );
}

#[test]
fn a_false_condition_excludes_only_that_stage() {
    let planned = decisions(&EvalContext::new("push", "refs/heads/topic", "abc"));
    assert_eq!(planned[0].1, Decision::Run);
    assert_eq!(planned[1].1, Decision::Run);
    assert_eq!(planned[2].1, Decision::Excluded);
}

#[test]
fn the_plan_lists_skipped_stages_rather_than_omitting_them() {
    // A report that silently drops them cannot answer "why did this not
    // deploy", which is the question conditions generate.
    let planned = decisions(&EvalContext::new("push", "refs/heads/topic", "abc"));
    assert_eq!(planned.len(), 3);
}

#[test]
fn exclusion_propagates_to_dependents() {
    let source = r#"
[[stage]]
name = "gate"
when = "branch == 'master'"
[[stage.job]]
steps = ["true"]

[[stage]]
name = "after"
needs = ["gate"]
[[stage.job]]
steps = ["true"]
"#;
    let spec = parse(source).expect("parses");
    let planned = plan(&spec, &EvalContext::new("push", "refs/heads/topic", "abc"));

    assert_eq!(planned[0].decision, Decision::Excluded);
    assert_eq!(
        planned[1].decision,
        Decision::Blocked {
            by: "gate".to_string()
        }
    );
}

#[test]
fn exclusion_propagates_transitively() {
    let source = r#"
[[stage]]
name = "gate"
when = "branch == 'master'"
[[stage.job]]
steps = ["true"]

[[stage]]
name = "middle"
needs = ["gate"]
[[stage.job]]
steps = ["true"]

[[stage]]
name = "last"
needs = ["middle"]
[[stage.job]]
steps = ["true"]
"#;
    let spec = parse(source).expect("parses");
    let planned = plan(&spec, &EvalContext::new("push", "refs/heads/topic", "abc"));

    assert!(!planned[1].decision.will_run());
    assert_eq!(
        planned[2].decision,
        Decision::Blocked {
            by: "middle".to_string()
        }
    );
}

#[test]
fn a_job_condition_is_evaluated_on_top_of_the_stages() {
    let source = r#"
[[stage]]
name = "build"
[[stage.job]]
name = "always"
steps = ["true"]
[[stage.job]]
name = "master-only"
when = "branch == 'master'"
steps = ["true"]
"#;
    let spec = parse(source).expect("parses");
    let planned = plan(&spec, &EvalContext::new("push", "refs/heads/topic", "abc"));

    assert_eq!(planned[0].decision, Decision::Run);
    assert_eq!(planned[0].jobs[0].decision, Decision::Run);
    assert_eq!(planned[0].jobs[1].decision, Decision::Excluded);
}

#[test]
fn jobs_of_an_excluded_stage_inherit_that_exclusion() {
    // Not `Blocked { by: "deploy" }`: a job told that its own stage did not run
    // learns nothing, and that is what the run report would have shown.
    let source = r#"
[[stage]]
name = "deploy"
when = "branch == 'master'"
[[stage.job]]
name = "push-image"
steps = ["true"]
"#;
    let spec = parse(source).expect("parses");
    let planned = plan(&spec, &EvalContext::new("push", "refs/heads/topic", "abc"));

    assert_eq!(planned[0].jobs[0].decision, Decision::Excluded);
    assert!(
        planned[0].jobs[0]
            .decision
            .reason()
            .is_some_and(|reason| reason.contains("when")),
        "the reason should point at the condition"
    );
}

#[test]
fn jobs_of_a_blocked_stage_name_the_stage_that_caused_it() {
    let source = r#"
[[stage]]
name = "gate"
when = "branch == 'master'"
[[stage.job]]
steps = ["true"]

[[stage]]
name  = "after"
needs = ["gate"]
[[stage.job]]
name = "deploy"
steps = ["true"]
"#;
    let spec = parse(source).expect("parses");
    let planned = plan(&spec, &EvalContext::new("push", "refs/heads/topic", "abc"));

    // 'gate', not 'after': the reason points at what actually caused it.
    assert_eq!(
        planned[1].jobs[0].decision,
        Decision::Blocked {
            by: "gate".to_string()
        }
    );
}

#[test]
fn a_job_condition_cannot_resurrect_an_excluded_stage() {
    let source = r#"
[[stage]]
name = "deploy"
when = "branch == 'master'"
[[stage.job]]
when = "event == 'push'"
steps = ["true"]
"#;
    let spec = parse(source).expect("parses");
    let planned = plan(&spec, &EvalContext::new("push", "refs/heads/topic", "abc"));

    assert!(!planned[0].jobs[0].decision.will_run());
}

#[test]
fn decisions_explain_themselves() {
    assert!(Decision::Run.reason().is_none());
    assert!(
        Decision::Excluded
            .reason()
            .is_some_and(|r| r.contains("when"))
    );
    assert!(
        Decision::Blocked {
            by: "build".to_string()
        }
        .reason()
        .is_some_and(|r| r.contains("build"))
    );
}
