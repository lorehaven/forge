use songbook_service::jobs::Jobs;
use std::time::Duration;

async fn wait(jobs: &Jobs) {
    for _ in 0..200 {
        if !jobs.running() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the job did not finish");
}

#[quench_test::test]
async fn one_job_at_a_time_with_a_log_read_from_an_offset() {
    let jobs = Jobs::default();
    assert!(jobs.read(0).name.is_none());
    let (release, hold) = tokio::sync::oneshot::channel::<()>();
    assert!(jobs.start("sync", |log| async move {
        log.line("first");
        let _ = hold.await;
        log.line("second");
        Ok(())
    }));
    assert!(
        !jobs.start("convert", |_| async { Ok(()) }),
        "a job is running"
    );
    tokio::time::sleep(Duration::from_millis(30)).await;
    let mid = jobs.read(0);
    assert_eq!(
        (mid.name.as_deref(), mid.running, mid.log.as_str()),
        (Some("sync"), true, "first\n")
    );
    release.send(()).unwrap();
    wait(&jobs).await;
    let rest = jobs.read(mid.offset);
    assert_eq!(
        (rest.log.as_str(), rest.running, rest.error),
        ("second\n", false, None)
    );
    assert_eq!(rest.offset, 13);
}

#[quench_test::test]
async fn a_failed_job_records_why_and_a_new_one_starts_a_new_log() {
    let jobs = Jobs::default();
    assert!(jobs.start("download", |log| async move {
        log.line("working");
        Err("yt-dlp not found".to_string())
    }));
    wait(&jobs).await;
    let failed = jobs.read(0);
    assert_eq!(failed.error.as_deref(), Some("yt-dlp not found"));
    assert!(failed.log.ends_with("failed: yt-dlp not found\n"));

    assert!(jobs.start("tags", |log| async move {
        log.line("x");
        Ok(())
    }));
    wait(&jobs).await;
    // a client still holding the old job's offset is sent back to the start
    let fresh = jobs.read(failed.offset);
    assert_eq!(
        (fresh.log.as_str(), fresh.error, fresh.offset),
        ("x\n", None, 2)
    );
}
