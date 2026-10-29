// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::unwrap_used, clippy::too_many_lines)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use hoist_app::live::{Msg, Request};
use hoist_desktop::{Store, package_formats, work, work_with};

/// What the stand-in services remember.
#[derive(Default)]
struct State {
    access: String,
    refresh: String,
    issued: u32,
    catalog_status: u16,
    file: Vec<u8>,
    requests: Vec<String>,
}

fn respond(stream: &mut TcpStream, status: &str, body: &[u8]) {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body);
}

fn serve(state: &Arc<Mutex<State>>, mut stream: TcpStream) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    let (method, path) = {
        let mut parts = line.split_whitespace();
        (
            parts.next().unwrap_or("").to_string(),
            parts.next().unwrap_or("").to_string(),
        )
    };
    let (mut length, mut auth) = (0usize, String::new());
    loop {
        let mut header = String::new();
        reader.read_line(&mut header).unwrap();
        if header.trim().is_empty() {
            break;
        }
        let lower = header.to_ascii_lowercase();
        if let Some(v) = lower.strip_prefix("content-length:") {
            length = v.trim().parse().unwrap();
        }
        if lower.starts_with("authorization:") {
            auth = header.split_once(':').unwrap().1.trim().to_string();
        }
    }
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body).unwrap();
    let body = String::from_utf8_lossy(&body).to_string();
    let mut s = state.lock().unwrap();
    s.requests.push(format!("{method} {path}"));
    let tokens = |s: &mut State| {
        s.issued += 1;
        s.access = format!("access-{}", s.issued);
        s.refresh = format!("refresh-{}", s.issued);
        format!(
            "{{\"access_token\":\"{}\",\"refresh_token\":\"{}\",\"token_type\":\"Bearer\",\"expires_in\":900}}",
            s.access, s.refresh
        )
    };
    match (method.as_str(), path.as_str()) {
        ("POST", "/gatehouse/api/v1/auth/login") if body.contains("\"mfa\"") => respond(
            &mut stream,
            "401 Unauthorized",
            b"{\"error\":\"mfa_required\"}",
        ),
        ("POST", "/gatehouse/api/v1/auth/login") if body.contains("\"hunter2\"") => {
            let t = tokens(&mut s);
            respond(&mut stream, "200 OK", t.as_bytes());
        }
        ("POST", "/gatehouse/api/v1/auth/login") => respond(
            &mut stream,
            "401 Unauthorized",
            b"{\"error\":\"invalid_credentials\"}",
        ),
        ("POST", "/gatehouse/api/v1/auth/refresh") => {
            // The refresh token rotates: only the latest is good.
            if body.contains(&format!("\"{}\"", s.refresh)) && !s.refresh.is_empty() {
                let t = tokens(&mut s);
                respond(&mut stream, "200 OK", t.as_bytes());
            } else {
                respond(&mut stream, "401 Unauthorized", b"{}");
            }
        }
        ("GET", p) if p.starts_with("/warehouse/api/v1/artifacts?") || p.contains("/download") => {
            if auth != format!("Bearer {}", s.access) {
                respond(&mut stream, "401 Unauthorized", b"{}");
            } else if p.contains("/download") {
                let file = s.file.clone();
                respond(&mut stream, "200 OK", &file);
            } else if s.catalog_status == 404 {
                respond(&mut stream, "404 Not Found", b"{}");
            } else {
                respond(&mut stream, "200 OK", ROWS.as_bytes());
            }
        }
        ("GET", p) if p.starts_with("/hoist/v1/") => {
            if auth != format!("Bearer {}", s.access) {
                respond(&mut stream, "401 Unauthorized", b"{}");
            } else if p.starts_with("/hoist/v1/categories") {
                respond(
                    &mut stream,
                    "200 OK",
                    br#"{"items":[{"name":"Games","count":3}]}"#,
                );
            } else {
                respond(
                    &mut stream,
                    "200 OK",
                    br#"{"total":1,"offset":0,"items":[{"package_name":"org.x.chess","label":"Chess","version_code":2,"download_url":"http://127.0.0.1:1/chess.apk"}]}"#,
                );
            }
        }
        _ => respond(&mut stream, "500 Internal Server Error", b"{}"),
    }
}

const ROWS: &str = r#"[{"program":"mathom","platform":"linux","arch":"universal","format":"deb","version_code":12,"version_name":"1.2.0","filename":"mathom_1.2.0.deb","size_bytes":5,"sha256":"HASH","label":"Mathom","uploaded_by":"ci","yanked":false,"created_at":"2026-10-28T12:00:00Z"}]"#;

fn start() -> (String, Arc<Mutex<State>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();
    let state = Arc::new(Mutex::new(State {
        file: b"hello".to_vec(),
        ..State::default()
    }));
    let shared = Arc::clone(&state);
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let state = Arc::clone(&shared);
            std::thread::spawn(move || serve(&state, stream));
        }
    });
    (format!("http://{addr}/"), state)
}

fn sha(bytes: &[u8]) -> String {
    quench_crypto::hash::sha256(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn ask(
    tx: &std::sync::mpsc::Sender<Request>,
    rx: &std::sync::mpsc::Receiver<Msg>,
    request: Request,
) -> Msg {
    tx.send(request).unwrap();
    rx.recv_timeout(Duration::from_secs(10)).unwrap()
}

#[quench_test::test]
fn the_worker_signs_in_keeps_the_rotating_token_lists_downloads_and_signs_out() {
    let (base, state) = start();
    let dir = std::env::temp_dir().join(format!("hoist-desktop-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let store = Store::at(dir.clone()).with_formats(vec!["deb"]);
    let (requests, requests_rx) = channel();
    let (answers, answers_rx) = channel();
    let worker = {
        let store = store.clone();
        std::thread::spawn(move || work(&requests_rx, &answers, &store))
    };
    assert_eq!(
        ask(&requests, &answers_rx, Request::Catalog),
        Msg::SignedOut,
        "nobody is signed in yet"
    );
    assert_eq!(
        ask(
            &requests,
            &answers_rx,
            Request::Login {
                server: base.clone(),
                username: "ann".into(),
                password: "nope".into()
            }
        ),
        Msg::LoginFailed("the user name or password is wrong, or the session ended".into())
    );
    assert_eq!(
        ask(
            &requests,
            &answers_rx,
            Request::Login {
                server: base.clone(),
                username: "ann".into(),
                password: "hunter2".into()
            }
        ),
        Msg::LoggedIn
    );
    assert_eq!(
        store.read().unwrap().1,
        "refresh-1",
        "the refresh token is kept, not the password"
    );
    let mode = std::os::unix::fs::PermissionsExt::mode(
        &std::fs::metadata(dir.join("session"))
            .unwrap()
            .permissions(),
    );
    assert_eq!(mode & 0o777, 0o600, "readable by this user only");
    let rows = match ask(&requests, &answers_rx, Request::Catalog) {
        Msg::Catalog(rows) => rows,
        other => panic!("{other:?}"),
    };
    assert_eq!(rows.len(), 1);
    let mut artifact = rows[0].clone();
    artifact.sha256 = sha(b"hello");
    // A server that forgets the token: the worker refreshes and keeps the rotated one.
    state.lock().unwrap().access = "gone".into();
    let Msg::Downloaded { program, path } =
        ask(&requests, &answers_rx, Request::Download(artifact.clone()))
    else {
        panic!("a download")
    };
    assert_eq!(program, "mathom");
    assert_eq!(std::fs::read(&path).unwrap(), b"hello");
    assert!(
        store.read().unwrap().1.starts_with("refresh-"),
        "{:?}",
        store.read()
    );
    assert_ne!(store.read().unwrap().1, "refresh-1");
    artifact.sha256 = sha(b"other");
    assert_eq!(
        ask(&requests, &answers_rx, Request::Download(artifact)),
        Msg::DownloadFailed("the download was corrupted, try again".into())
    );
    state.lock().unwrap().catalog_status = 404;
    assert_eq!(
        ask(&requests, &answers_rx, Request::Catalog),
        Msg::CatalogFailed("this server has no registry".into())
    );
    assert_eq!(ask(&requests, &answers_rx, Request::Logout), Msg::SignedOut);
    assert!(store.read().is_none(), "the stored session is gone");
    assert_eq!(
        ask(&requests, &answers_rx, Request::Download(rows[0].clone())),
        Msg::SignedOut
    );
    drop(requests);
    worker.join().unwrap();
    // A later run picks the stored session up.
    store.write(&base, "refresh-0");
    assert_eq!(store.read(), Some((base, "refresh-0".to_string())));
    assert!(Store::standard().downloads().ends_with("hoist/downloads"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[quench_test::test]
fn installing_downloads_hands_the_file_to_the_system_and_keeps_a_ledger() {
    let (base, state) = start();
    let dir = std::env::temp_dir().join(format!("hoist-install-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let store = Store::at(dir.clone()).with_formats(vec!["deb"]);
    assert!(store.installed().is_empty());
    store.write(&base, "refresh-0");
    // The stand-in accepts the stored token once, as the first refresh.
    state.lock().unwrap().refresh = "refresh-0".into();
    let (requests, requests_rx) = channel();
    let (answers, answers_rx) = channel();
    let opened = Arc::new(Mutex::new(Vec::<String>::new()));
    let worker = {
        let (store, opened) = (store.clone(), Arc::clone(&opened));
        std::thread::spawn(move || {
            work_with(&requests_rx, &answers, &store, &|path| {
                opened.lock().unwrap().push(path.display().to_string());
                if path.to_string_lossy().ends_with("refuse") {
                    return Err("no installer".to_string());
                }
                Ok(())
            });
        })
    };
    let Msg::Catalog(rows) = ask(&requests, &answers_rx, Request::Catalog) else {
        panic!("a catalog")
    };
    let mut artifact = rows[0].clone();
    artifact.sha256 = sha(b"hello");
    let Msg::Installed {
        program,
        version_code,
        version_name,
        path,
    } = ask(&requests, &answers_rx, Request::Install(artifact.clone()))
    else {
        panic!("installed")
    };
    assert_eq!(
        (program.as_str(), version_code, version_name.as_str()),
        ("mathom", 12, "1.2.0")
    );
    assert_eq!(
        *opened.lock().unwrap(),
        std::slice::from_ref(&path),
        "the downloaded file was opened"
    );
    assert_eq!(std::fs::read(&path).unwrap(), b"hello");
    // The ledger survives a restart, one line per program.
    let ledger = store.installed();
    assert_eq!(ledger["mathom"].version_code, 12);
    store.record(&hoist_core::Installed {
        program: "daybook".into(),
        version_code: 3,
        version_name: "0.3".into(),
    });
    store.record(&hoist_core::Installed {
        program: "mathom".into(),
        version_code: 13,
        version_name: "1.3\t0".into(),
    });
    let all = Store::at(dir.clone()).installed();
    assert_eq!(all.len(), 2);
    assert_eq!(
        (
            all["mathom"].version_code,
            all["mathom"].version_name.as_str()
        ),
        (13, "1.3 0")
    );
    // A file the system will not open is reported with where it is, and is not on the ledger.
    let mut refused = artifact.clone();
    refused.program = "refuse".into();
    refused.filename = "refuse".into();
    let Msg::DownloadFailed(why) = ask(&requests, &answers_rx, Request::Install(refused)) else {
        panic!("a failure")
    };
    assert!(
        why.contains("no installer") && why.contains("refuse"),
        "{why}"
    );
    assert!(!store.installed().contains_key("refuse"));
    // A corrupted download is not opened at all.
    artifact.sha256 = sha(b"other");
    let before = opened.lock().unwrap().len();
    assert!(matches!(
        ask(&requests, &answers_rx, Request::Install(artifact)),
        Msg::DownloadFailed(_)
    ));
    assert_eq!(opened.lock().unwrap().len(), before);
    drop(requests);
    worker.join().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[quench_test::test]
fn the_worker_reads_the_public_catalogue_and_refuses_what_it_should_not_fetch() {
    let (base, _state) = start();
    let dir = std::env::temp_dir().join(format!("hoist-public-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let store = Store::at(dir.clone()).with_formats(vec!["deb"]);
    store.write(&base, "refresh-0");
    let (requests, requests_rx) = channel();
    let (answers, answers_rx) = channel();
    let worker = {
        let store = store.clone();
        std::thread::spawn(move || work(&requests_rx, &answers, &store))
    };
    assert_eq!(
        ask(
            &requests,
            &answers_rx,
            Request::Login {
                server: base.clone(),
                username: "ann".into(),
                password: "hunter2".into()
            }
        ),
        Msg::LoggedIn
    );
    let Msg::Categories(rows) = ask(&requests, &answers_rx, Request::PublicCategories) else {
        panic!("categories")
    };
    assert_eq!((rows[0].name.as_str(), rows[0].count), ("Games", 3));
    let asked = hoist_core::PublicQuery {
        category: Some("Games".into()),
        sort: "rating".into(),
        limit: 10,
        ..hoist_core::PublicQuery::default()
    };
    let Msg::PublicPage {
        asked: echoed,
        page,
    } = ask(&requests, &answers_rx, Request::PublicPage(asked.clone()))
    else {
        panic!("a page")
    };
    assert_eq!((echoed, page.items[0].title()), (asked, "Chess"));
    // Its address is not one that is fetched (nothing listens there, and it is not https either).
    let app = page.items[0].clone();
    let mut remote = app.clone();
    remote.download_url = "http://example.org/chess.apk".into();
    assert!(matches!(
        ask(&requests, &answers_rx, Request::DownloadPublic(remote)),
        Msg::DownloadFailed(_)
    ));
    // Installing is the phone's business: nothing is answered for it here.
    requests.send(Request::InstallPublic(app)).unwrap();
    assert_eq!(ask(&requests, &answers_rx, Request::Logout), Msg::SignedOut);
    drop(requests);
    worker.join().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[quench_test::test]
fn preferences_are_kept_and_a_new_server_address_carries_the_session() {
    let (base, _state) = start();
    let dir = std::env::temp_dir().join(format!("hoist-prefs-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let store = Store::at(dir.clone()).with_formats(vec!["deb"]);
    assert!(store.auto_check(), "on until it is switched off");
    store.write(&base, "refresh-0");
    let (requests, requests_rx) = channel();
    let (answers, answers_rx) = channel();
    let worker = {
        let store = store.clone();
        std::thread::spawn(move || work(&requests_rx, &answers, &store))
    };
    requests
        .send(Request::Preferences {
            server: "http://other.example:9/".into(),
            auto_check: false,
        })
        .unwrap();
    // Nothing is answered for it: the next request is.
    assert_eq!(ask(&requests, &answers_rx, Request::Logout), Msg::SignedOut);
    assert!(!store.auto_check());
    drop(requests);
    worker.join().unwrap();
    // The session file moved to the new address before it was cleared by signing out; the setting stays.
    assert!(Store::at(dir.clone()).read().is_none());
    store.save_preferences(true);
    assert!(Store::at(dir.clone()).auto_check());
    // A session that is still there keeps its token at the new address.
    store.write(&base, "refresh-0");
    let (requests, requests_rx) = channel();
    let (answers, _answers_rx) = channel();
    let worker = {
        let store = store.clone();
        std::thread::spawn(move || work(&requests_rx, &answers, &store))
    };
    requests
        .send(Request::Preferences {
            server: "http://other.example:9/".into(),
            auto_check: true,
        })
        .unwrap();
    drop(requests);
    worker.join().unwrap();
    assert_eq!(
        store.read(),
        Some((
            "http://other.example:9/".to_string(),
            "refresh-0".to_string()
        ))
    );
    let _ = std::fs::remove_dir_all(&dir);
}

quench_test::main!();

#[quench_test::test]
fn the_package_format_follows_the_distribution() {
    for (os_release, want) in [
        ("NAME=\"Arch Linux\"\nID=arch\n", "pkg.tar.zst"),
        ("ID=endeavouros\nID_LIKE=arch\n", "pkg.tar.zst"),
        ("ID=ubuntu\nID_LIKE=debian\n", "deb"),
        ("ID=debian\n", "deb"),
        ("ID=fedora\n", "rpm"),
        ("ID=\"rocky\"\nID_LIKE=\"rhel centos fedora\"\n", "rpm"),
        ("ID=opensuse-tumbleweed\nID_LIKE=\"opensuse suse\"\n", "rpm"),
        ("ID=nixos\n", "tar.gz"),
        ("", "tar.gz"),
    ] {
        assert_eq!(package_formats(os_release), [want], "{os_release:?}");
    }
}
