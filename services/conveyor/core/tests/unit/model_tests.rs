use crate::support::{at, run};
use conveyor_core::{Access, RunDetail, Status};
use quench_serde::json;

#[quench_test::test]
fn a_run_detail_reads_back_with_its_flattened_run() {
    let text = r#"{"id":"r1","repo_id":"x","trigger":"manual","git_ref":"refs/heads/master","sha":"abcdef0123","message":null,
        "delivery_id":null,"status":"running","queued_at":"2026-10-29T10:00:00Z","started_at":null,"finished_at":null,
        "claimed_by":null,"claimed_at":null,"attempt":1,"error":null,"resumed_from":null,"stages":["check"],
        "jobs":[{"id":"j1","run_id":"r1","stage":"check","name":"lint","needs":[],"status":"success","exit_code":0,
        "started_at":null,"finished_at":null,"error":null,"reused_from_run":null}],
        "artifacts":[{"id":"a1","run_id":"r1","job_id":"j1","kind":"file","name":"f","version":null,"uri":"u","digest":null,
        "created_at":"2026-10-29T10:00:00Z","href":"https://h/f"}]}"#;
    let detail: RunDetail = json::from_str(text).unwrap();
    assert_eq!(detail.run.status, Status::Running);
    assert_eq!(detail.run.short_sha(), "abcdef0");
    assert_eq!(detail.run.ref_name(), "master");
    assert_eq!(detail.jobs[0].qualified_name(), "check/lint");
    assert_eq!(detail.artifacts[0].href, "https://h/f");
    assert_eq!(detail.artifacts[0].artifact.name, "f");
    assert_eq!(
        json::from_str::<RunDetail>(&json::to_string(&detail)).unwrap(),
        detail
    );
}

#[quench_test::test]
fn access_answers_per_project() {
    let access = Access {
        username: "ola".into(),
        read: vec!["a".into(), "b".into()],
        write: vec!["a".into()],
    };
    assert!(access.can_read("b") && !access.can_read("c"));
    assert!(access.can_write("a") && !access.can_write("b"));
}

#[quench_test::test]
fn statuses_know_their_state() {
    assert!(Status::Failed.is_failure() && Status::Cancelled.is_failure());
    assert!(!Status::Skipped.is_failure());
    assert!(Status::Skipped.is_terminal() && !Status::Running.is_terminal());
    let r = run("r", "x", Status::Success);
    assert_eq!(r.queued_at, at("2026-10-29T10:00:00Z"));
}
