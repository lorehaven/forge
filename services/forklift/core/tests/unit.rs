use forklift_core::{
    Engine, Folder, Found, LARGE_FILE_BYTES, MAX_ATTEMPTS, Manifest, Outcome, Pause, Settings,
    Status, Verdict, backoff_ms, clean_server, folder_slug, remote_path, sanitize, storage_name,
    year_month,
};

#[quench_test::test]
fn the_path_has_category_date_id_and_name() {
    assert_eq!(
        remote_path("photos", "123", "IMG_0001.jpg", 2026, 3),
        "photos/2026/03/123-IMG_0001.jpg"
    );
}

#[quench_test::test]
fn unsafe_characters_become_underscores() {
    assert_eq!(sanitize("a b/c\u{e9}"), "a_b_c_");
    assert_eq!(
        remote_path("custom:docs", "a/b", "my file.pdf", 2026, 12),
        "custom:docs/2026/12/a_b-my_file.pdf"
    );
}

#[quench_test::test]
fn an_empty_segment_is_an_underscore() {
    assert_eq!(sanitize(""), "_");
}

#[quench_test::test]
fn the_month_comes_from_the_time() {
    assert_eq!(year_month(0), (1970, 1));
    assert_eq!(year_month(1_709_251_199_000), (2024, 2)); // 2024-02-29T23:59:59Z
    assert_eq!(year_month(1_709_251_200_000), (2024, 3));
    assert_eq!(year_month(-1), (1969, 12));
    assert_eq!(year_month(1_798_761_600_000), (2027, 1));
}

fn found(id: &str, size: u64, modified: i64) -> Found {
    Found {
        local_id: id.into(),
        name: format!("{id}.jpg"),
        uri: format!("content://media/{id}"),
        size,
        modified_ms: modified,
    }
}

#[quench_test::test]
fn a_new_file_waits_and_an_unchanged_one_is_left_alone() {
    let mut m = Manifest::new();
    let s = m.scan("photos", &[found("1", 10, 100), found("2", 20, 200)], false);
    assert_eq!((s.added, s.changed, s.unchanged), (2, 0, 0));
    m.done("photos", "1", "photos/x");
    let s = m.scan("photos", &[found("1", 10, 100), found("2", 20, 200)], false);
    assert_eq!((s.added, s.changed, s.unchanged), (0, 0, 2));
    assert_eq!(m.get("photos", "1").unwrap().status, Status::Done);
}

#[quench_test::test]
fn a_changed_file_waits_again_from_the_start() {
    let mut m = Manifest::new();
    m.scan("photos", &[found("1", 10, 100)], false);
    m.done("photos", "1", "photos/x");
    let s = m.scan("photos", &[found("1", 11, 100)], false);
    assert_eq!(s.changed, 1);
    let item = m.get("photos", "1").unwrap();
    assert_eq!(
        (item.status, item.attempts, item.size),
        (Status::Pending, 0, 11)
    );
}

#[quench_test::test]
fn the_same_id_in_two_categories_is_two_files() {
    let mut m = Manifest::new();
    m.scan("photos", &[found("1", 10, 100)], false);
    m.scan("custom:docs", &[found("1", 10, 100)], false);
    assert_eq!(m.len(), 2);
}

#[quench_test::test]
fn failures_back_off_and_end_in_failed() {
    let mut m = Manifest::new();
    m.scan("photos", &[found("1", 10, 100)], false);
    m.start("photos", "1");
    m.failed("photos", "1", 1_000, "timeout");
    let i = m.get("photos", "1").unwrap();
    assert_eq!(
        (i.status, i.attempts, i.retry_at_ms),
        (Status::Pending, 1, 1_000 + 30_000)
    );
    assert!(m.next_batch(1_000, 10, true).is_empty());
    assert_eq!(m.next_due_ms(1_000), Some(31_000));
    assert_eq!(m.next_batch(31_000, 10, true).len(), 1);
    for _ in 1..MAX_ATTEMPTS {
        m.failed("photos", "1", 0, "timeout");
    }
    assert_eq!(m.get("photos", "1").unwrap().status, Status::Failed);
    // An automatic scan leaves it; a scan the person asked for tries again.
    m.scan("photos", &[found("1", 10, 100)], false);
    assert_eq!(m.get("photos", "1").unwrap().status, Status::Failed);
    m.scan("photos", &[found("1", 10, 100)], true);
    let i = m.get("photos", "1").unwrap();
    assert_eq!((i.status, i.attempts), (Status::Pending, 0));
}

#[quench_test::test]
fn the_wait_doubles_to_an_hour() {
    assert_eq!(backoff_ms(1), 30_000);
    assert_eq!(backoff_ms(2), 60_000);
    assert_eq!(backoff_ms(3), 120_000);
    assert_eq!(backoff_ms(40), 3_600_000);
}

#[quench_test::test]
fn the_newest_goes_first_and_large_files_wait_for_the_charger() {
    let mut m = Manifest::new();
    m.scan(
        "photos",
        &[
            found("a", 10, 100),
            found("b", 10, 300),
            found("c", LARGE_FILE_BYTES + 1, 400),
        ],
        false,
    );
    let ids: Vec<&str> = m
        .next_batch(0, 10, false)
        .iter()
        .map(|i| i.local_id.as_str())
        .collect();
    assert_eq!(ids, ["b", "a"]);
    let ids: Vec<&str> = m
        .next_batch(0, 2, true)
        .iter()
        .map(|i| i.local_id.as_str())
        .collect();
    assert_eq!(ids, ["c", "b"]);
}

#[quench_test::test]
fn a_file_found_sending_after_a_crash_waits_again() {
    let mut m = Manifest::new();
    m.scan("photos", &[found("1", 10, 100)], false);
    m.start("photos", "1");
    let mut loaded = Manifest::from_text(&m.to_text());
    loaded.recover();
    assert_eq!(loaded.get("photos", "1").unwrap().status, Status::Pending);
}

#[quench_test::test]
fn the_text_round_trips_awkward_names() {
    let mut m = Manifest::new();
    let mut f = found("1", 10, 100);
    f.name = "tab\there\nnew\\line.jpg".into();
    m.scan("custom:my docs", &[f], false);
    m.failed("custom:my docs", "1", 5, "bad\tthing");
    let back = Manifest::from_text(&m.to_text());
    assert_eq!(back, m);
}

#[quench_test::test]
fn a_damaged_line_costs_only_itself() {
    let mut m = Manifest::new();
    m.scan("photos", &[found("1", 10, 100), found("2", 20, 200)], false);
    let text = m.to_text().replacen("pending", "bogus", 1);
    assert_eq!(Manifest::from_text(&text).len(), 1);
}

#[quench_test::test]
fn totals_count_by_status_and_bytes() {
    let mut m = Manifest::new();
    m.scan(
        "photos",
        &[
            found("1", 10, 100),
            found("2", 20, 200),
            found("3", 30, 300),
        ],
        false,
    );
    m.done("photos", "1", "x");
    m.too_large("photos", "3");
    let t = m.totals(None);
    assert_eq!(
        (
            t.pending,
            t.done,
            t.too_large,
            t.done_bytes,
            t.waiting_bytes,
            t.waiting()
        ),
        (1, 1, 1, 10, 20, 1)
    );
    assert_eq!(m.totals(Some("nothing")).done, 0);
}

#[quench_test::test]
fn forgetting_a_category_keeps_the_others() {
    let mut m = Manifest::new();
    m.scan("photos", &[found("1", 10, 100)], false);
    m.scan("custom:a", &[found("1", 10, 100)], false);
    m.forget_category("custom:a");
    assert_eq!(m.len(), 1);
}

fn engine(n: usize) -> Engine {
    let mut m = Manifest::new();
    let files: Vec<Found> = (1..=n)
        .map(|i| found(&i.to_string(), 10, i as i64 * 1000))
        .collect();
    m.scan("photos", &files, false);
    Engine::new(m, 2)
}

#[quench_test::test]
fn the_engine_runs_as_many_as_it_may_and_the_newest_first() {
    let mut e = engine(5);
    let starts = e.pump(0, false);
    assert_eq!(starts.len(), 2);
    assert_eq!(starts[0].uri, "content://media/5");
    assert_eq!(starts[0].remote, "photos/1970/01/5-5.jpg");
    assert!(e.pump(0, false).is_empty());
    assert_eq!(e.running(), 2);
    assert_eq!(
        e.finish(&starts[0].upload_id, &Outcome::Http(200), 0),
        Verdict::Done
    );
    assert_eq!(e.pump(0, false).len(), 1);
    assert_eq!(e.manifest.get("photos", "5").unwrap().status, Status::Done);
}

#[quench_test::test]
fn an_edited_file_goes_to_the_path_it_first_had() {
    let mut e = engine(1);
    let first = e.pump(0, false).remove(0);
    e.finish(&first.upload_id, &Outcome::Http(201), 0);
    e.manifest
        .scan("photos", &[found("1", 99, 9_999_999_999_999)], false);
    assert_eq!(e.pump(0, false)[0].remote, first.remote);
}

#[quench_test::test]
fn a_server_error_counts_as_a_failure_and_a_broken_transfer_too() {
    let mut e = engine(2);
    let starts = e.pump(0, false);
    assert_eq!(
        e.finish(&starts[0].upload_id, &Outcome::Http(500), 10),
        Verdict::Retry
    );
    assert_eq!(
        e.finish(&starts[1].upload_id, &Outcome::Failed("reset".into()), 10),
        Verdict::Retry
    );
    let i = e.manifest.get("photos", "2").unwrap();
    assert_eq!(
        (i.status, i.attempts, i.error.as_str()),
        (Status::Pending, 1, "the server answered 500")
    );
    assert!(e.pump(10, false).is_empty());
}

#[quench_test::test]
fn a_full_storage_pauses_without_counting_a_failure() {
    let mut e = engine(2);
    let starts = e.pump(0, false);
    assert_eq!(
        e.finish(&starts[0].upload_id, &Outcome::Http(507), 0),
        Verdict::Paused(Pause::Quota)
    );
    assert!(e.pump(0, false).is_empty());
    assert_eq!(e.manifest.get("photos", "2").unwrap().attempts, 0);
    e.stop();
    assert_eq!(
        e.manifest.get("photos", "1").unwrap().status,
        Status::Pending
    );
    e.resume();
    assert_eq!(e.pump(0, false).len(), 2);
}

#[quench_test::test]
fn too_large_is_final_for_that_file() {
    let mut e = engine(1);
    let s = e.pump(0, false).remove(0);
    e.finish(&s.upload_id, &Outcome::Http(413), 0);
    assert_eq!(
        e.manifest.get("photos", "1").unwrap().status,
        Status::TooLarge
    );
    assert!(e.pump(0, false).is_empty());
}

#[quench_test::test]
fn a_refused_token_gets_a_second_chance_then_signs_out() {
    let mut e = engine(1);
    let s = e.pump(0, false).remove(0);
    assert_eq!(
        e.finish(&s.upload_id, &Outcome::Http(401), 0),
        Verdict::Reauthenticate
    );
    let s = e.pump(0, false).remove(0);
    assert_eq!(
        e.finish(&s.upload_id, &Outcome::Http(401), 0),
        Verdict::Reauthenticate
    );
    let s = e.pump(0, false).remove(0);
    assert_eq!(
        e.finish(&s.upload_id, &Outcome::Http(401), 0),
        Verdict::Paused(Pause::SignedOut)
    );
    assert_eq!(e.manifest.get("photos", "1").unwrap().attempts, 0);
}

#[quench_test::test]
fn a_success_clears_the_refusals() {
    let mut e = engine(2);
    let starts = e.pump(0, false);
    e.finish(&starts[0].upload_id, &Outcome::Http(401), 0);
    e.finish(&starts[1].upload_id, &Outcome::Http(200), 0);
    let s = e.pump(0, false).remove(0);
    assert_eq!(
        e.finish(&s.upload_id, &Outcome::Http(401), 0),
        Verdict::Reauthenticate
    );
}

#[quench_test::test]
fn the_settings_round_trip() {
    let mut s = Settings {
        server: "https://h.example".into(),
        storage: "moto".into(),
        storage_confirmed: true,
        auto: false,
        concurrency: 4,
        ..Settings::default()
    };
    s.folders.push(Folder {
        slug: "docs".into(),
        name: "My\tdocs".into(),
        uri: "content://tree/x".into(),
        enabled: true,
        wifi_only: true,
    });
    assert_eq!(Settings::from_text(&s.to_text()), s);
}

#[quench_test::test]
fn an_unknown_line_or_a_wild_number_does_no_harm() {
    let s = Settings::from_text("future=1\nconcurrency=99\nphotos=0\nnonsense");
    assert_eq!((s.concurrency, s.photos, s.videos), (5, false, true));
}

#[quench_test::test]
fn a_storage_is_named_from_the_model() {
    assert_eq!(storage_name("Moto Edge 60 Pro"), "moto_edge_60_pro");
    assert_eq!(storage_name("  Pixel-8 (Emulator) "), "pixel_8_emulator");
    assert_eq!(storage_name("???"), "device");
}

#[quench_test::test]
fn a_folder_slug_is_unique() {
    let f = |slug: &str| Folder {
        slug: slug.into(),
        name: String::new(),
        uri: String::new(),
        enabled: true,
        wifi_only: false,
    };
    assert_eq!(folder_slug("Documents", &[]), "documents");
    assert_eq!(folder_slug("Documents", &[f("documents")]), "documents_2");
    assert_eq!(
        folder_slug("Documents", &[f("documents"), f("documents_2")]),
        "documents_3"
    );
}

#[quench_test::test]
fn the_categories_follow_the_switches() {
    let mut s = Settings {
        videos_wifi_only: true,
        ..Settings::default()
    };
    s.folders.push(Folder {
        slug: "a".into(),
        name: "A".into(),
        uri: "u".into(),
        enabled: true,
        wifi_only: false,
    });
    s.folders.push(Folder {
        slug: "b".into(),
        name: "B".into(),
        uri: "v".into(),
        enabled: false,
        wifi_only: false,
    });
    let keys: Vec<(String, bool)> = s
        .categories()
        .into_iter()
        .map(|c| (c.key, c.wifi_only))
        .collect();
    assert_eq!(
        keys,
        [
            ("photos".to_string(), false),
            ("videos".to_string(), true),
            ("custom:a".to_string(), false)
        ]
    );
    s.photos = false;
    assert_eq!(s.categories().len(), 2);
}

#[quench_test::test]
fn a_server_address_is_cleaned() {
    assert_eq!(clean_server(" https://h.example// "), "https://h.example");
}

fn snap(n: usize, done: usize, working: bool) -> forklift_core::Snapshot {
    let mut e = engine(n);
    for i in 1..=done {
        e.manifest.done("photos", &i.to_string(), "x");
    }
    forklift_core::Snapshot::of(&e, &Settings::default(), working, None)
}

#[quench_test::test]
fn the_headline_says_where_the_backup_is() {
    assert_eq!(snap(0, 0, false).headline(), "Nothing to back up yet");
    assert_eq!(snap(0, 0, true).headline(), "Looking for files");
    assert_eq!(snap(4, 1, true).headline(), "Backing up: 1 of 4 files");
    assert_eq!(snap(4, 4, false).headline(), "All 4 files are backed up");
}

#[quench_test::test]
fn failed_files_are_said_after_the_waiting_ones() {
    let mut e = engine(2);
    e.manifest.done("photos", "1", "x");
    for _ in 0..forklift_core::MAX_ATTEMPTS {
        e.manifest.failed("photos", "2", 0, "no");
    }
    let s = forklift_core::Snapshot::of(&e, &Settings::default(), false, None);
    assert_eq!(s.headline(), "1 file could not be backed up");
    assert!((s.fraction() - 0.5).abs() < 1e-9);
}

#[quench_test::test]
fn a_pause_is_the_headline() {
    let mut e = engine(1);
    let s = e.pump(0, false).remove(0);
    e.finish(&s.upload_id, &Outcome::Http(507), 0);
    let shot = forklift_core::Snapshot::of(&e, &Settings::default(), false, None);
    assert!(shot.headline().starts_with("the storage is full"));
}

#[quench_test::test]
fn the_snapshot_names_what_is_sending() {
    let mut e = engine(3);
    e.pump(0, false);
    let s = forklift_core::Snapshot::of(&e, &Settings::default(), true, None);
    let mut names = s.sending;
    names.sort();
    assert_eq!(names, ["2.jpg", "3.jpg"]);
}

#[quench_test::test]
fn sizes_and_times_are_readable() {
    assert_eq!(forklift_core::bytes(950), "950 B");
    assert_eq!(forklift_core::bytes(12_500), "12.5 KB");
    assert_eq!(forklift_core::bytes(3_400_000_000), "3.4 GB");
    assert_eq!(forklift_core::clock(0, 0), "00:00");
    assert_eq!(
        forklift_core::clock(3_600_000 * 5 + 60_000 * 7, 120),
        "07:07"
    );
    assert_eq!(forklift_core::clock(0, -60), "23:00");
}

#[quench_test::test]
fn a_category_kept_for_wifi_waits_on_mobile_data() {
    let mut e = engine(2);
    e.manifest.scan("videos", &[found("v", 10, 5_000)], false);
    let starts = e.pump_where(0, false, |category| category != "videos");
    assert_eq!(starts.len(), 2);
    assert!(starts.iter().all(|s| !s.uri.ends_with("/v")));
}

#[quench_test::test]
fn a_short_id_is_stable_and_tells_names_apart() {
    assert_eq!(forklift_core::short_id("a"), "af63dc4c8601ec8c");
    assert_ne!(
        forklift_core::short_id("content://x/1"),
        forklift_core::short_id("content://x/2")
    );
    assert_eq!(forklift_core::short_id("content://x/1").len(), 16);
}

fn remote(path: &str, size: u64) -> forklift_core::Remote {
    forklift_core::Remote {
        path: path.into(),
        size,
    }
}

#[quench_test::test]
fn a_stored_files_category_and_name_come_from_its_path() {
    assert_eq!(
        forklift_core::category_of("custom:docs/2026/03/ab12-report.pdf"),
        "custom:docs"
    );
    assert_eq!(
        forklift_core::base_name("photos/2026/03/123-IMG_1-copy.jpg"),
        "IMG_1-copy.jpg"
    );
}

#[quench_test::test]
fn a_fresh_install_does_not_send_what_the_server_holds() {
    let mut m = Manifest::new();
    m.scan(
        "photos",
        &[
            found("1", 10, 1_000),
            found("2", 20, 2_000),
            found("3", 30, 3_000),
        ],
        false,
    );
    // The server has "1" at its path, "2" under another id (the device renumbered it) and "3" with another size.
    let known = [
        remote("photos/1970/01/1-1.jpg", 10),
        remote("photos/1969/12/77-2.jpg", 20),
        remote("photos/1970/01/3-3.jpg", 31),
    ];
    assert_eq!(m.reconcile(&known), 2);
    let one = m.get("photos", "1").unwrap();
    assert_eq!(
        (one.status, one.remote.as_str()),
        (Status::Done, "photos/1970/01/1-1.jpg")
    );
    assert_eq!(
        m.get("photos", "2").unwrap().remote,
        "photos/1969/12/77-2.jpg"
    );
    assert_eq!(
        m.get("photos", "3").unwrap().status,
        Status::Pending,
        "a different size is a different file"
    );
}

#[quench_test::test]
fn reconciling_leaves_files_already_sent_or_failed_alone() {
    let mut m = Manifest::new();
    m.scan(
        "photos",
        &[found("1", 10, 1_000), found("2", 10, 1_000)],
        false,
    );
    m.failed("photos", "2", 0, "no");
    m.done("photos", "1", "photos/elsewhere");
    assert_eq!(
        m.reconcile(&[
            remote("photos/1970/01/1-1.jpg", 10),
            remote("photos/1970/01/2-2.jpg", 10)
        ]),
        0
    );
    assert_eq!(m.get("photos", "1").unwrap().remote, "photos/elsewhere");
}

#[quench_test::test]
fn a_restore_brings_back_what_the_device_lacks_and_has_a_place_for() {
    let mut m = Manifest::new();
    m.scan("photos", &[found("1", 10, 1_000)], false);
    m.done("photos", "1", "photos/1970/01/1-1.jpg");
    let folders = [Folder {
        slug: "docs".into(),
        name: "Docs".into(),
        uri: "u".into(),
        enabled: true,
        wifi_only: false,
    }];
    let stored = [
        remote("photos/1970/01/1-1.jpg", 10),
        remote("photos/1970/02/9-gone.jpg", 5),
        remote("videos/1970/02/8-clip.mp4", 50),
        remote("custom:docs/1970/01/ab-a.pdf", 7),
        remote("custom:old/1970/01/cd-b.pdf", 7),
    ];
    let plan = forklift_core::plan(&stored, &m.present(), &folders);
    let paths: Vec<&str> = plan.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(
        paths,
        [
            "photos/1970/02/9-gone.jpg",
            "videos/1970/02/8-clip.mp4",
            "custom:docs/1970/01/ab-a.pdf"
        ]
    );
    assert_eq!((plan.bytes, plan.present, plan.no_target), (62, 1, 1));
}

#[quench_test::test]
fn a_file_restored_under_a_new_id_is_not_taken_for_missing_nor_sent_again() {
    // It was restored: the device gave it id 99. The scan finds it; the server holds it as 5-trip.jpg.
    let mut m = Manifest::new();
    m.scan(
        "photos",
        &[Found {
            local_id: "99".into(),
            name: "trip.jpg".into(),
            uri: "u".into(),
            size: 12,
            modified_ms: 5_000,
        }],
        false,
    );
    let stored = [remote("photos/1969/01/5-trip.jpg", 12)];
    assert!(
        forklift_core::plan(&stored, &m.present(), &[])
            .files
            .is_empty()
    );
    assert_eq!(m.reconcile(&stored), 1);
}

#[quench_test::test]
fn the_restorer_runs_a_few_at_a_time_retries_and_gives_up() {
    let files: Vec<forklift_core::Remote> =
        (1..=4).map(|i| remote(&format!("photos/{i}"), 1)).collect();
    let mut r = forklift_core::Restorer::new(files, 2);
    let first = r.start();
    assert_eq!(
        first
            .iter()
            .map(|f| f.file.path.as_str())
            .collect::<Vec<_>>(),
        ["photos/1", "photos/2"]
    );
    assert!(r.start().is_empty(), "two at a time");
    r.finish(&first[0].id, true);
    // photos/2 fails and is tried again before the files that have not been tried at all.
    r.finish(&first[1].id, false);
    let next = r.start();
    assert_eq!(
        next.iter()
            .map(|f| f.file.path.as_str())
            .collect::<Vec<_>>(),
        ["photos/2", "photos/3"]
    );
    assert_eq!(r.progress().done, 1);
}

#[quench_test::test]
fn a_file_failing_every_time_is_counted_failed_and_the_run_ends() {
    let mut r = forklift_core::Restorer::new(vec![remote("photos/1", 1)], 1);
    for _ in 0..forklift_core::RESTORE_ATTEMPTS {
        let fetch = r.start().remove(0);
        assert!(!r.finished());
        r.finish(&fetch.id, false);
    }
    assert!(r.finished());
    let p = r.progress();
    assert_eq!((p.total, p.done, p.failed), (1, 0, 1));
}

#[quench_test::test]
fn stopping_a_restore_gives_up_what_runs_and_starts_nothing() {
    let mut r = forklift_core::Restorer::new(
        (1..=3).map(|i| remote(&format!("photos/{i}"), 1)).collect(),
        2,
    );
    let started = r.start();
    let cancelled = r.stop();
    assert_eq!(cancelled.len(), started.len());
    assert!(r.start().is_empty());
    assert!(r.finished());
}

#[quench_test::test]
fn what_a_scan_no_longer_finds_is_gone_from_the_device_and_a_restore_brings_it_back() {
    let mut m = Manifest::new();
    m.scan(
        "photos",
        &[
            found("1", 10, 1_000),
            found("2", 20, 2_000),
            found("3", 30, 3_000),
        ],
        false,
    );
    m.done("photos", "1", "photos/1970/01/1-1.jpg");
    m.done("photos", "2", "photos/1970/01/2-2.jpg");
    // "2" was deleted from the device; "3" was too, before it was ever sent.
    m.mark_missing("photos", &[found("1", 10, 1_000)]);
    assert!(
        m.get("photos", "2").unwrap().gone,
        "kept, as the record of the copy"
    );
    assert!(
        m.get("photos", "3").is_none(),
        "never sent, so nothing to keep"
    );
    let stored = [
        remote("photos/1970/01/1-1.jpg", 10),
        remote("photos/1970/01/2-2.jpg", 20),
    ];
    let plan = forklift_core::plan(&stored, &m.present(), &[]);
    assert_eq!(plan.files, [remote("photos/1970/01/2-2.jpg", 20)]);
    // It comes back (the same id), and is on the device again.
    m.scan(
        "photos",
        &[found("1", 10, 1_000), found("2", 20, 2_000)],
        false,
    );
    assert!(!m.get("photos", "2").unwrap().gone);
    assert!(
        forklift_core::plan(&stored, &m.present(), &[])
            .files
            .is_empty()
    );
}

#[quench_test::test]
fn the_manifest_of_an_older_version_is_still_read() {
    let old = "forklift-manifest 1\nphotos\t1\ta.jpg\tu\t10\t100\tdone\t0\t0\t\tphotos/x\n";
    let m = Manifest::from_text(old);
    assert_eq!(m.len(), 1);
    assert!(!m.get("photos", "1").unwrap().gone);
}

#[quench_test::test]
fn a_storage_not_yet_chosen_is_the_default_of_an_older_settings_file() {
    let s = Settings::from_text("server=https://h\nstorage=moto\nauto=1\n");
    assert_eq!((s.storage.as_str(), s.storage_confirmed), ("moto", false));
}

#[quench_test::test]
fn a_refusal_of_the_account_is_not_taken_for_an_expired_token() {
    let mut e = engine(1);
    let s = e.pump(0, false).remove(0);
    assert_eq!(
        e.finish(&s.upload_id, &Outcome::Http(403), 0),
        Verdict::Paused(Pause::Forbidden)
    );
    assert!(Pause::Forbidden.words().contains("gatehouse"));
    assert_eq!(
        e.manifest.get("photos", "1").unwrap().attempts,
        0,
        "not counted against the file"
    );
    assert!(e.pump(0, false).is_empty());
}

quench_test::main!();

#[quench_test::test]
fn the_counts_of_every_category_come_from_one_walk_and_agree_with_the_one_by_one_counts() {
    let mut m = Manifest::new();
    for category in ["photos", "videos", "custom:docs"] {
        let files: Vec<Found> = (1..=7)
            .map(|i| found(&format!("{category}{i}"), i * 10, i as i64))
            .collect();
        m.scan(category, &files, false);
    }
    m.done("videos", "videos1", "x");
    m.failed("custom:docs", "custom:docs2", 0, "no");
    m.too_large("photos", "photos3");
    let by = m.totals_by_category();
    assert_eq!(by.len(), 3);
    for (category, totals) in &by {
        assert_eq!(*totals, m.totals(Some(category)), "{category}");
    }
}

#[quench_test::test]
fn the_next_batch_is_the_newest_few_of_very_many_waiting_in_order() {
    let mut m = Manifest::new();
    // Scrambled times, so that the waiting ones are not in order of age by any accident.
    let files: Vec<Found> = (0..5000_i64)
        .map(|i| found(&i.to_string(), 1, (i * 7919) % 5003))
        .collect();
    m.scan("photos", &files, false);
    let mut expected: Vec<(i64, String)> = m
        .items()
        .map(|i| (i.modified_ms, i.local_id.clone()))
        .collect();
    expected.sort_by(|a, b| b.cmp(a));
    for n in [0, 1, 3, 50] {
        let got: Vec<(i64, String)> = m
            .next_batch(0, n, true)
            .into_iter()
            .map(|i| (i.modified_ms, i.local_id.clone()))
            .collect();
        assert_eq!(got, expected[..n], "{n}");
    }
}

#[quench_test::test]
fn a_scan_handed_over_in_pages_forgets_what_is_gone_like_one_handed_over_whole() {
    let all: Vec<Found> = (1..=6).map(|i| found(&i.to_string(), 10, i)).collect();
    let mut paged = Manifest::new();
    let mut whole = Manifest::new();
    paged.scan("photos", &all, false);
    whole.scan("photos", &all, false);
    paged.done("photos", "2", "x");
    whole.done("photos", "2", "x");
    // 2 (sent) and 4 (not sent) vanish from the device.
    let now: Vec<Found> = all
        .iter()
        .filter(|f| f.local_id != "2" && f.local_id != "4")
        .cloned()
        .collect();
    for page in now.chunks(2) {
        paged.scan("photos", page, false);
    }
    let seen: std::collections::BTreeSet<&str> = now.iter().map(|f| f.local_id.as_str()).collect();
    paged.mark_missing_ids("photos", &seen);
    whole.scan("photos", &now, false);
    whole.mark_missing("photos", &now);
    assert_eq!(paged, whole);
    assert!(paged.get("photos", "2").unwrap().gone, "sent: kept, gone");
    assert!(paged.get("photos", "4").is_none(), "never sent: forgotten");
}

#[quench_test::test]
fn what_the_dashboard_showed_is_written_and_read_back() {
    let mut e = engine(5);
    e.manifest.done("photos", "1", "x");
    e.manifest.done("photos", "2", "x");
    e.manifest.too_large("photos", "3");
    let mut shot =
        forklift_core::Snapshot::of(&e, &Settings::default(), true, Some(1_700_000_000_000));
    shot.quota = Some((12, 3456));
    let back = forklift_core::Snapshot::from_text(&shot.to_text());
    assert_eq!(back.totals, shot.totals);
    assert_eq!(back.rows, shot.rows);
    assert_eq!(back.checked_ms, shot.checked_ms);
    assert_eq!(back.quota, shot.quota);
    assert_eq!(back.headline(), "Backing up: 2 of 4 files");
    // Damage gives the empty one, never a half-read one.
    assert_eq!(
        forklift_core::Snapshot::from_text("junk"),
        forklift_core::Snapshot::default()
    );
    assert_eq!(
        forklift_core::Snapshot::from_text("forklift-snapshot 1\ntotal 1 2 x"),
        forklift_core::Snapshot::default()
    );
}
