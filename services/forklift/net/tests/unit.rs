// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::unwrap_used, clippy::too_many_lines)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

use forklift_net::{NetError, Session, login, upload_url};

/// What a stand-in server saw of one request.
#[derive(Clone, Debug, Default)]
struct Seen {
    method: String,
    target: String,
    auth: String,
    content_type: String,
    body: Vec<u8>,
}

type Handler = Box<dyn Fn(&Seen) -> (u16, Vec<(String, String)>, String) + Send + Sync>;

fn serve(handler: Handler) -> (String, Arc<Mutex<Vec<Seen>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();
    let log = Arc::new(Mutex::new(Vec::new()));
    let (kept, handler) = (Arc::clone(&log), Arc::new(handler));
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let (log, handler) = (Arc::clone(&kept), Arc::clone(&handler));
            std::thread::spawn(move || answer(stream, &log, &handler));
        }
    });
    (format!("http://{addr}"), log)
}

fn answer(mut stream: TcpStream, log: &Mutex<Vec<Seen>>, handler: &Handler) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    let mut parts = line.split_whitespace();
    let mut seen = Seen {
        method: parts.next().unwrap().into(),
        target: parts.next().unwrap().into(),
        ..Seen::default()
    };
    let mut length = 0usize;
    loop {
        let mut header = String::new();
        reader.read_line(&mut header).unwrap();
        if header.trim().is_empty() {
            break;
        }
        let (name, value) = header.split_once(':').unwrap();
        let value = value.trim().to_string();
        match name.to_ascii_lowercase().as_str() {
            "content-length" => length = value.parse().unwrap(),
            "authorization" => seen.auth = value,
            "content-type" => seen.content_type = value,
            _ => {}
        }
    }
    seen.body = vec![0; length];
    reader.read_exact(&mut seen.body).unwrap();
    let (status, headers, body) = handler(&seen);
    log.lock().unwrap().push(seen);
    let mut head = format!(
        "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    for (k, v) in headers {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str("\r\n");
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body.as_bytes());
}

fn tokens(n: u32, expires: i64) -> String {
    format!(
        "{{\"access_token\":\"access-{n}\",\"refresh_token\":\"refresh-{n}\",\"token_type\":\"Bearer\",\"expires_in\":{expires}}}"
    )
}

/// A gatehouse that hands out numbered tokens, and a warehouse that answers `files` with `files`.
fn stand_in(
    files: impl Fn(&Seen) -> (u16, Vec<(String, String)>, String) + Send + Sync + 'static,
) -> (String, Arc<Mutex<Vec<Seen>>>) {
    let issued = Arc::new(Mutex::new(0u32));
    serve(Box::new(move |seen| {
        if seen.target.starts_with("/gatehouse/api/v1/auth/") {
            let mut n = issued.lock().unwrap();
            *n += 1;
            return (200, vec![], tokens(*n, 900));
        }
        files(seen)
    }))
}

#[quench_test::test]
fn the_url_encodes_the_storage_and_the_path() {
    assert_eq!(
        upload_url(
            "https://h.example/",
            "moto edge",
            "photos/2026/03/1-a b.jpg"
        ),
        "https://h.example/warehouse/api/v1/files/moto%20edge/file?path=photos%2F2026%2F03%2F1-a%20b.jpg"
    );
}

#[quench_test::test]
fn login_then_a_fresh_token_is_the_one_from_the_login() {
    let (base, log) = stand_in(|_| (404, vec![], String::new()));
    let mut s = login(&base, "u", "p").unwrap();
    assert_eq!(s.fresh_token().unwrap(), "access-1");
    assert_eq!(
        log.lock().unwrap().len(),
        1,
        "no refresh while the token has life"
    );
}

#[quench_test::test]
fn a_token_at_the_end_of_its_life_is_refreshed_and_the_new_refresh_token_is_kept() {
    let issued = Arc::new(Mutex::new(0u32));
    let counter = Arc::clone(&issued);
    let (base, _) = serve(Box::new(move |_| {
        let mut n = counter.lock().unwrap();
        *n += 1;
        // Born already stale: a life of 0 s.
        (200, vec![], tokens(*n, 0))
    }));
    let kept = Arc::new(Mutex::new(Vec::<String>::new()));
    let sink = Arc::clone(&kept);
    let mut s = login(&base, "u", "p")
        .unwrap()
        .keeping_tokens(move |t| sink.lock().unwrap().push(t.to_string()));
    assert_eq!(s.fresh_token().unwrap(), "access-2");
    assert_eq!(*kept.lock().unwrap(), ["refresh-2"]);
    assert_eq!(s.refresh_token(), "refresh-2");
}

#[quench_test::test]
fn the_storages_come_with_quota_and_use() {
    let (base, log) = stand_in(|_| {
        (200, vec![], r#"[{"name":"static-one"},{"name":"moto","owner":"me","quota_bytes":1000,"used_bytes":250}]"#.into())
    });
    let mut s = Session::resume(&base, "r");
    let storages = s.storages().unwrap();
    assert_eq!(storages.len(), 2);
    // Only a storage someone made has an owner.
    assert_eq!(
        (storages[0].owner.as_deref(), storages[1].owner.as_deref()),
        (None, Some("me"))
    );
    assert_eq!(
        (
            storages[0].quota_bytes,
            storages[1].quota_bytes,
            storages[1].used_bytes
        ),
        (None, Some(1000), Some(250))
    );
    let seen = log.lock().unwrap().last().unwrap().clone();
    assert_eq!(
        (
            seen.method.as_str(),
            seen.target.as_str(),
            seen.auth.as_str()
        ),
        ("GET", "/warehouse/api/v1/files", "Bearer access-1")
    );
}

#[quench_test::test]
fn a_listing_pages_by_the_link_header() {
    let (base, log) = stand_in(|seen| {
        if seen.target.contains("last=") {
            (
                200,
                vec![],
                r#"{"entries":[{"name":"c","path":"p/c","kind":"file","size":3}]}"#.into(),
            )
        } else {
            (
                200,
                vec![("Link".into(), "<x>; rel=\"next\"".into())],
                r#"{"entries":[{"name":"a","path":"p/a","kind":"file","size":1},{"name":"b","path":"p/b","kind":"file"}]}"#.into(),
            )
        }
    });
    let mut s = Session::resume(&base, "r");
    let first = s.list("moto", "p/", 2, None).unwrap();
    assert_eq!(
        (first.entries.len(), first.next.as_deref()),
        (2, Some("p/b"))
    );
    let second = s.list("moto", "p/", 2, first.next.as_deref()).unwrap();
    assert_eq!((second.entries.len(), second.next), (1, None));
    let target = log
        .lock()
        .unwrap()
        .iter()
        .map(|s| s.target.clone())
        .collect::<Vec<_>>();
    assert!(
        target
            .iter()
            .any(|t| t.starts_with("/warehouse/api/v1/files/moto?")
                && t.contains("prefix=p%2F")
                && t.contains("n=2")),
        "{target:?}"
    );
}

#[quench_test::test]
fn the_change_feed_is_read_from_a_cursor() {
    let (base, log) = stand_in(|_| {
        (200, vec![], r#"[{"id":5,"path":"photos/a","op":"put","size":9},{"id":6,"path":"photos/b","op":"delete"}]"#.into())
    });
    let mut s = Session::resume(&base, "r");
    let changes = s.changes("moto", 4).unwrap();
    assert_eq!(
        (
            changes.len(),
            changes[0].id,
            changes[1].op.as_str(),
            changes[1].size
        ),
        (2, 5, "delete", None)
    );
    assert!(
        log.lock()
            .unwrap()
            .last()
            .unwrap()
            .target
            .ends_with("/moto/sync?since=4")
    );
}

#[quench_test::test]
fn a_file_is_put_whole_with_the_bearer_and_its_path() {
    let (base, log) = stand_in(|_| {
        (
            200,
            vec![],
            r#"{"path":"p","size":3,"digest":"sha256:x"}"#.into(),
        )
    });
    let mut s = Session::resume(&base, "r");
    s.put("moto", "photos/2026/03/1-a.jpg", b"abc".to_vec())
        .unwrap();
    let seen = log.lock().unwrap().last().unwrap().clone();
    assert_eq!(seen.method, "PUT");
    assert_eq!(
        seen.target,
        "/warehouse/api/v1/files/moto/file?path=photos%2F2026%2F03%2F1-a.jpg"
    );
    assert_eq!(
        (seen.body.as_slice(), seen.content_type.as_str()),
        (&b"abc"[..], "application/octet-stream")
    );
    assert_eq!(seen.auth, "Bearer access-1");
}

#[quench_test::test]
fn the_server_s_refusals_are_told_apart() {
    for (status, expected) in [
        (413, NetError::TooLarge),
        (507, NetError::QuotaFull),
        (403, NetError::Forbidden),
        (404, NetError::NoStorage),
        (500, NetError::Status(500)),
    ] {
        let (base, _) = stand_in(move |_| (status, vec![], String::new()));
        let mut s = Session::resume(&base, "r");
        assert_eq!(
            s.put("moto", "p", vec![1]).unwrap_err(),
            expected,
            "status {status}"
        );
    }
}

#[quench_test::test]
fn a_refused_token_is_refreshed_once_and_the_put_is_made_again() {
    let first = Arc::new(Mutex::new(true));
    let (base, log) = stand_in(move |_| {
        let mut f = first.lock().unwrap();
        if std::mem::take(&mut *f) {
            (401, vec![], String::new())
        } else {
            (200, vec![], "{}".into())
        }
    });
    let mut s = Session::resume(&base, "r");
    s.put("moto", "p", vec![1]).unwrap();
    let puts: Vec<_> = log
        .lock()
        .unwrap()
        .iter()
        .filter(|s| s.method == "PUT")
        .map(|s| s.auth.clone())
        .collect();
    assert_eq!(puts, ["Bearer access-1", "Bearer access-2"]);
}

#[quench_test::test]
fn a_stored_file_comes_back_whole() {
    let (base, _) = stand_in(|_| (200, vec![], "bytes".into()));
    let mut s = Session::resume(&base, "r");
    assert_eq!(s.get_file("moto", "p").unwrap(), b"bytes");
}

#[quench_test::test]
fn a_wrong_password_is_unauthorized_and_a_locked_account_is_told() {
    let (base, _) = serve(Box::new(|_| {
        (401, vec![], r#"{"error":"account_locked"}"#.into())
    }));
    assert_eq!(login(&base, "u", "p").unwrap_err(), NetError::AccountLocked);
    let (base, _) = serve(Box::new(|_| (401, vec![], r#"{"error":"nope"}"#.into())));
    assert_eq!(login(&base, "u", "p").unwrap_err(), NetError::Unauthorized);
}

fn temp(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("forklift-net-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[quench_test::test]
fn the_store_keeps_the_session_private_and_gives_it_back() {
    use std::os::unix::fs::PermissionsExt;
    let dir = temp("session");
    let store = forklift_net::Store::new(&dir);
    assert_eq!(store.token(), None);
    store.set_token("refresh-1");
    store.set_token("refresh-2");
    assert_eq!(store.token().as_deref(), Some("refresh-2"));
    assert_eq!(
        std::fs::metadata(dir.join("session"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    store.clear_token();
    assert_eq!(store.token(), None);
    let _ = std::fs::remove_dir_all(dir);
}

#[quench_test::test]
fn the_store_round_trips_the_settings_and_the_manifest_and_recovers_sending_files() {
    let dir = temp("data");
    let store = forklift_net::Store::new(&dir);
    assert_eq!(store.settings(), forklift_core::Settings::default());
    let settings = forklift_core::Settings {
        storage: "moto".into(),
        auto: false,
        ..Default::default()
    };
    store.set_settings(&settings);
    assert_eq!(store.settings(), settings);
    let mut manifest = forklift_core::Manifest::new();
    manifest.scan(
        "photos",
        &[forklift_core::Found {
            local_id: "1".into(),
            name: "a.jpg".into(),
            uri: "u".into(),
            size: 1,
            modified_ms: 2,
        }],
        false,
    );
    manifest.start("photos", "1");
    store.set_manifest(&manifest);
    assert_eq!(
        store.manifest().get("photos", "1").unwrap().status,
        forklift_core::Status::Pending
    );
    store.set_checked_ms(42);
    assert_eq!(store.checked_ms(), Some(42));
    assert!(
        !dir.join("manifest.tmp").exists(),
        "the temporary file is renamed away"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[quench_test::test]
fn a_background_write_never_puts_an_old_manifest_over_a_newer_one() {
    let dir = temp("order");
    let store = forklift_net::Store::new(&dir);
    let with = |n: usize| {
        let mut m = forklift_core::Manifest::new();
        let files: Vec<forklift_core::Found> = (0..n)
            .map(|i| forklift_core::Found {
                local_id: i.to_string(),
                name: "a.jpg".into(),
                uri: "u".into(),
                size: 1,
                modified_ms: 2,
            })
            .collect();
        m.scan("photos", &files, false);
        m
    };
    // The background write is asked for first, the plain one second: whatever order the threads run in, the plain
    // one is what is on the disk in the end.
    for _ in 0..20 {
        store.set_manifest_in_background(&with(1));
        store.set_manifest(&with(2));
        std::thread::sleep(std::time::Duration::from_millis(5));
        assert_eq!(store.manifest().len(), 2);
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[quench_test::test]
fn what_the_dashboard_showed_comes_back_from_the_store() {
    let dir = temp("snapshot");
    let store = forklift_net::Store::new(&dir);
    assert_eq!(store.snapshot(), forklift_core::Snapshot::default());
    let mut shot = forklift_core::Snapshot::default();
    shot.totals.done = 7;
    shot.checked_ms = Some(5);
    store.set_snapshot(&shot);
    assert_eq!(store.snapshot().totals.done, 7);
    store.clear_snapshot();
    assert_eq!(store.snapshot(), forklift_core::Snapshot::default());
    let _ = std::fs::remove_dir_all(dir);
}

#[quench_test::test]
fn only_one_program_may_run_the_backup() {
    let dir = temp("lock");
    let store = forklift_net::Store::new(&dir);
    let first = store.try_lock_run().expect("free at first");
    assert!(store.try_lock_run().is_none(), "held");
    drop(first);
    assert!(store.try_lock_run().is_some(), "free again once released");
    let _ = std::fs::remove_dir_all(dir);
}

#[quench_test::test]
fn the_whole_listing_is_walked_page_by_page_keeping_only_files() {
    let (base, log) = stand_in(|seen| {
        if seen.target.contains("last=p%2Fb") {
            (
                200,
                vec![],
                r#"{"entries":[{"name":"c","path":"p/c","kind":"file","size":3}]}"#.into(),
            )
        } else {
            (
                200,
                vec![("Link".into(), "<x>; rel=\"next\"".into())],
                r#"{"entries":[{"name":"a","path":"p/a","kind":"file","size":1},{"name":"d","path":"p/d","kind":"dir"},{"name":"b","path":"p/b","kind":"file","size":2}]}"#.into(),
            )
        }
    });
    let mut s = Session::resume(&base, "r");
    let all = s.list_all("moto", "").unwrap();
    assert_eq!(
        all.iter().map(|e| e.path.as_str()).collect::<Vec<_>>(),
        ["p/a", "p/b", "p/c"]
    );
    assert_eq!(
        log.lock()
            .unwrap()
            .iter()
            .filter(|s| s.target.starts_with("/warehouse/api/v1/files/moto?"))
            .count(),
        2
    );
}

#[quench_test::test]
fn the_sync_cursor_is_kept_per_storage() {
    let dir = temp("cursor");
    let store = forklift_net::Store::new(&dir);
    assert_eq!(store.sync_cursor("moto"), 0);
    store.set_sync_cursor("moto", 42);
    store.set_sync_cursor("other", 7);
    assert_eq!(
        (store.sync_cursor("moto"), store.sync_cursor("other")),
        (42, 7)
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[quench_test::test]
fn a_file_is_sent_and_fetched_with_a_token_the_caller_already_holds() {
    let (base, log) = stand_in(|seen| {
        if seen.method == "PUT" {
            (200, vec![], "{}".into())
        } else {
            (200, vec![], "bytes".into())
        }
    });
    let url = forklift_net::upload_url(&base, "moto", "p/a b.jpg");
    assert_eq!(
        forklift_net::put_with_token(&url, "tok", b"abc".to_vec()).unwrap(),
        200
    );
    let seen = log.lock().unwrap().last().unwrap().clone();
    assert_eq!(
        (
            seen.method.as_str(),
            seen.auth.as_str(),
            seen.body.as_slice()
        ),
        ("PUT", "Bearer tok", &b"abc"[..])
    );
    assert_eq!(
        forklift_net::get_with_token(&url, "tok").unwrap(),
        (200, b"bytes".to_vec())
    );
    // A refusal is an answer, not a failure of the call.
    let (base, _) = stand_in(|_| (507, vec![], String::new()));
    assert_eq!(
        forklift_net::put_with_token(&forklift_net::upload_url(&base, "m", "p"), "t", vec![1])
            .unwrap(),
        507
    );
    assert_eq!(
        forklift_net::get_with_token(&forklift_net::upload_url(&base, "m", "p"), "t").unwrap(),
        (507, Vec::new())
    );
}

fn library(n: usize, prefix: &str) -> forklift_core::Manifest {
    let mut m = forklift_core::Manifest::new();
    let files: Vec<forklift_core::Found> = (0..n)
        .map(|i| forklift_core::Found {
            local_id: format!("{prefix}{i:06}"),
            name: format!("IMG_{i}.jpg"),
            uri: format!("content://media/external/images/media/{i}"),
            size: 1_000 + i as u64,
            modified_ms: 1_700_000_000_000 + i as i64,
        })
        .collect();
    m.scan("photos", &files, false);
    m
}

#[quench_test::test]
fn the_manifest_is_a_database_and_the_old_text_file_is_read_once() {
    let dir = temp("legacy");
    let mut old = library(50, "a");
    old.start("photos", "a000003");
    old.done("photos", "a000003", "photos/2023/IMG_3.jpg");
    old.failed("photos", "a000004", 1_000, "no route to host\tand a tab");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("manifest"), old.to_text()).unwrap();

    let store = forklift_net::Store::new(&dir);
    let loaded = store.manifest();
    assert_eq!(loaded.len(), 50);
    assert_eq!(loaded.get("photos", "a000003").unwrap().remote, "photos/2023/IMG_3.jpg");
    assert_eq!(loaded.get("photos", "a000004").unwrap().error, "no route to host\tand a tab");
    assert!(dir.join("manifest.db").exists());
    assert!(!dir.join("manifest").exists(), "the text file was read and set aside");
    assert!(dir.join("manifest.imported").exists());

    // The database is now the manifest: the old file, if it came back, would not be read again.
    let mut edited = loaded;
    edited.start("photos", "a000010");
    edited.done("photos", "a000010", "photos/2023/IMG_10.jpg");
    store.set_manifest(&edited);
    std::fs::write(dir.join("manifest"), old.to_text()).unwrap();
    let again = store.manifest();
    assert_eq!(again.get("photos", "a000010").unwrap().status, forklift_core::Status::Done);
    assert_eq!(again.len(), 50);
    let _ = std::fs::remove_dir_all(dir);
}

#[quench_test::test]
fn a_save_writes_only_what_changed() {
    let dir = temp("delta");
    let store = forklift_net::Store::new(&dir);
    let mut manifest = library(3_000, "f");
    store.set_manifest(&manifest);
    let db = dir.join("manifest.db");
    let first = std::fs::read(&db).unwrap();

    // The same manifest again: not a byte of the file changes.
    store.set_manifest(&manifest);
    assert_eq!(std::fs::read(&db).unwrap(), first, "an unchanged manifest writes nothing");

    // One file sent: a few pages change, the rest of the file is as it was.
    manifest.start("photos", "f001500");
    manifest.done("photos", "f001500", "photos/2023/IMG_1500.jpg");
    store.set_manifest(&manifest);
    let second = std::fs::read(&db).unwrap();
    assert_eq!(second.len(), first.len(), "no growth for one changed record");
    let pages = |a: &[u8], b: &[u8]| a.chunks(4096).zip(b.chunks(4096)).filter(|(x, y)| x != y).count();
    let changed = pages(&first, &second);
    assert!((1..=6).contains(&changed), "{changed} of {} pages changed", first.len() / 4096);

    // A file that is no longer in the manifest is gone from the database.
    manifest.forget_category("photos");
    store.set_manifest(&manifest);
    assert_eq!(store.manifest().len(), 0);
    let _ = std::fs::remove_dir_all(dir);
}

#[quench_test::test]
fn the_counts_file_tells_the_android_host_what_waits() {
    let dir = temp("counts");
    let store = forklift_net::Store::new(&dir);
    let mut manifest = library(10, "c");
    manifest.start("photos", "c000001");
    manifest.done("photos", "c000001", "photos/2023/IMG_1.jpg");
    manifest.start("photos", "c000002");
    for _ in 0..5 {
        manifest.failed("photos", "c000003", 1_000, "no route");
    }
    store.set_manifest(&manifest);
    let text = std::fs::read_to_string(dir.join("manifest.counts")).unwrap();
    assert_eq!(text, "forklift-counts 1\npending 7\nuploading 1\nfailed 1\n");
    // The text of an older manifest is counted into the same file when it is imported.
    let dir = temp("counts-legacy");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("manifest"), library(4, "l").to_text()).unwrap();
    assert_eq!(forklift_net::Store::new(&dir).manifest().len(), 4);
    assert_eq!(
        std::fs::read_to_string(dir.join("manifest.counts")).unwrap(),
        "forklift-counts 1\npending 4\nuploading 0\nfailed 0\n"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[quench_test::test]
fn a_big_library_round_trips() {
    let dir = temp("big");
    let store = forklift_net::Store::new(&dir);
    let mut manifest = library(20_000, "b");
    for i in (0..20_000).step_by(7) {
        let id = format!("b{i:06}");
        manifest.start("photos", &id);
        manifest.done("photos", &id, &format!("photos/2023/IMG_{i}.jpg"));
    }
    store.set_manifest(&manifest);
    let back = store.manifest();
    assert_eq!(back, manifest);
    assert_eq!(back.totals(None).pending + back.totals(None).uploading, manifest.totals(None).pending + manifest.totals(None).uploading);
    let _ = std::fs::remove_dir_all(dir);
}

#[quench_test::test]
fn a_hundred_thousand_records_save_and_load_in_seconds() {
    let dir = temp("hundred-thousand");
    let store = forklift_net::Store::new(&dir);
    let mut manifest = library(100_000, "h");
    let t = std::time::Instant::now();
    store.set_manifest(&manifest);
    let first_save = t.elapsed();
    let size = std::fs::metadata(dir.join("manifest.db")).unwrap().len();

    for i in 0..100 {
        let id = format!("h{:06}", i * 997);
        manifest.start("photos", &id);
        manifest.done("photos", &id, &format!("photos/2023/IMG_{i}.jpg"));
    }
    let t = std::time::Instant::now();
    store.set_manifest(&manifest);
    let small_save = t.elapsed();

    let t = std::time::Instant::now();
    let loaded = store.manifest();
    let load = t.elapsed();
    eprintln!(
        "100,000 records: first save {first_save:?}, save of 100 changes {small_save:?}, load {load:?}, file {} MiB",
        size / (1024 * 1024)
    );
    assert_eq!(loaded.len(), 100_000);
    assert_eq!(loaded, manifest);
    // Generous bounds for a slow machine; the point is seconds, not minutes.
    assert!(first_save < std::time::Duration::from_secs(60), "{first_save:?}");
    assert!(small_save < std::time::Duration::from_secs(10), "{small_save:?}");
    assert!(load < std::time::Duration::from_secs(20), "{load:?}");
    let _ = std::fs::remove_dir_all(dir);
}

#[quench_test::test]
fn a_manifest_that_cannot_be_read_is_never_overwritten_by_an_empty_one() {
    let dir = temp("unreadable");
    let store = forklift_net::Store::new(&dir);
    store.set_manifest(&library(20, "u"));
    // Damage: the first page is not a database header any more.
    let db = dir.join("manifest.db");
    let mut bytes = std::fs::read(&db).unwrap();
    bytes[..16].copy_from_slice(b"not a database!!");
    std::fs::write(&db, &bytes).unwrap();
    assert_eq!(store.manifest().len(), 0, "nothing can be read");
    store.set_manifest(&forklift_core::Manifest::new());
    assert_eq!(std::fs::read(&db).unwrap(), bytes, "and nothing was written over what is there");
    let _ = std::fs::remove_dir_all(dir);
}

#[quench_test::test]
fn another_program_holding_the_database_for_a_moment_is_waited_for() {
    let dir = temp("busy");
    let store = forklift_net::Store::new(&dir);
    store.set_manifest(&library(10, "w"));
    let held = quench_sqlite::Connection::open(dir.join("manifest.db")).unwrap();
    let release = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(300));
        drop(held);
    });
    let started = std::time::Instant::now();
    assert_eq!(store.manifest().len(), 10, "the read waited for the other program, then got the records");
    assert!(started.elapsed() >= std::time::Duration::from_millis(250));
    release.join().unwrap();
    let _ = std::fs::remove_dir_all(dir);
}

quench_test::main!();
