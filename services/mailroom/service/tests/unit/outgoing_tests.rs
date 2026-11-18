//! Uploads and the small rules of sending.

use crate::support::*;
use mailroom_service::error::Error;
use mailroom_service::outgoing::{
    MAX_UPLOAD, clean_filename, clean_media_type, display_name, service_name, upload,
};

macro_rules! db {
    () => {
        if !have_database() {
            return;
        }
    };
}

#[quench_test::test]
fn file_names_and_types_are_made_safe() {
    for (given, clean) in [
        ("report.pdf", "report.pdf"),
        ("../../etc/passwd", "passwd"),
        ("C:\\temp\\a.txt", "a.txt"),
        ("a\r\nb\".txt", "ab.txt"),
        (".hidden", "hidden"),
        ("   ", "attachment"),
        ("", "attachment"),
        ("<x>|?*", "x"),
    ] {
        assert_eq!(clean_filename(given), clean, "{given:?}");
    }
    assert_eq!(clean_filename(&"é".repeat(300)).chars().count(), 120);
    for (given, clean) in [
        ("text/plain; charset=utf-8", "text/plain"),
        ("IMAGE/PNG", "image/png"),
        ("application/vnd.ms-excel", "application/vnd.ms-excel"),
        ("nonsense", "application/octet-stream"),
        ("a/b c", "application/octet-stream"),
        ("/b", "application/octet-stream"),
        ("a/", "application/octet-stream"),
        ("", "application/octet-stream"),
    ] {
        assert_eq!(clean_media_type(given), clean, "{given:?}");
    }
    assert_eq!(
        clean_media_type(&format!("a/{}", "b".repeat(200))),
        "application/octet-stream"
    );
}

#[quench_test::test]
fn a_service_writes_under_its_own_name() {
    assert_eq!(service_name("gatehouse-mailroom"), "gatehouse");
    assert_eq!(service_name("  Conveyor-Mailroom "), "conveyor");
    assert_eq!(service_name("dev"), "dev");
    assert_eq!(service_name("-mailroom"), "service");
    assert_eq!(service_name("we ird"), "service");
    assert_eq!(service_name(""), "service");
    assert_eq!(display_name("gatehouse"), "Gatehouse");
    assert_eq!(display_name(""), "");
}

#[quench_test::test]
async fn uploads_are_bounded_per_file_and_per_person() {
    db!();
    let env = env().await;
    env.people(&["ann", "bob"]).await;
    let ok = upload(&env.app, "ann", "a.bin", "application/zip", vec![1; 1000])
        .await
        .unwrap();
    assert_eq!((ok.filename.as_str(), ok.size), ("a.bin", 1000));
    for bad in [vec![], vec![0; MAX_UPLOAD + 1]] {
        assert!(matches!(
            upload(&env.app, "ann", "x", "a/b", bad).await.unwrap_err(),
            Error::Invalid(_)
        ));
    }
    // Three 20 MB files fit under the 60 MB cap with the first; the next does not. Another person has their own room.
    for _ in 0..2 {
        upload(&env.app, "ann", "big", "a/b", vec![1; MAX_UPLOAD])
            .await
            .unwrap();
    }
    let e = upload(&env.app, "ann", "big", "a/b", vec![1; MAX_UPLOAD])
        .await
        .unwrap_err();
    assert!(
        matches!(&e, Error::Invalid(m) if m.contains("too many files")),
        "{e:?}"
    );
    upload(&env.app, "bob", "big", "a/b", vec![1; MAX_UPLOAD])
        .await
        .unwrap();
    // Parallel uploads cannot each pass the cap.
    let racers: Vec<_> = (0..4)
        .map(|_| {
            let app = env.app.clone();
            tokio::spawn(async move {
                upload(&app, "bob", "r", "a/b", vec![1; MAX_UPLOAD])
                    .await
                    .is_ok()
            })
        })
        .collect();
    let mut stored = 0;
    for r in racers {
        stored += usize::from(r.await.unwrap());
    }
    assert_eq!(stored, 2, "60 MB cap, 20 MB already there");
    // A disabled or non-mailbox name cannot upload.
    assert!(upload(&env.app, "a b", "x", "a/b", vec![1]).await.is_err());
}

#[quench_test::test]
async fn uploads_are_rate_limited() {
    db!();
    let env = env().await;
    env.people(&["ann"]).await;
    for _ in 0..60 {
        upload(&env.app, "ann", "a", "a/b", vec![1]).await.unwrap();
    }
    assert!(matches!(
        upload(&env.app, "ann", "a", "a/b", vec![1])
            .await
            .unwrap_err(),
        Error::Limited(_)
    ));
}
