// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::unwrap_used, clippy::too_many_lines)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

use hoist_core::Artifact;
use hoist_net::{NetError, Session, login};

/// What the stand-in services remember.
#[derive(Default)]
struct State {
    access: String,
    refresh: String,
    issued: u32,
    catalog_status: u16,
    /// What the stand-in answers on Entmoot (0 is `200`).
    public_status: u16,
    /// Whether the last file from the store came with an `Authorization` header.
    store_auth: Option<bool>,
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
            } else if s.public_status == 404 {
                respond(&mut stream, "404 Not Found", b"{}");
            } else if p.starts_with("/hoist/v1/categories") {
                respond(&mut stream, "200 OK", CATEGORIES.as_bytes());
            } else {
                respond(&mut stream, "200 OK", PAGE.as_bytes());
            }
        }
        // The store the public catalogue points at: no sign-in.
        ("GET", p) if p.starts_with("/store/") => {
            s.store_auth = Some(!auth.is_empty());
            let file = s.file.clone();
            respond(&mut stream, "200 OK", &file);
        }
        _ => respond(&mut stream, "500 Internal Server Error", b"{}"),
    }
}

const CATEGORIES: &str =
    r#"{"items":[{"name":"Games","count":40},{"name":"Internet","count":12}]}"#;
const PAGE: &str = r#"{"total":3,"offset":0,"limit":2,"items":[{"package_name":"org.example.maps","label":"Maps","category":"Navigation","version_code":7,"version_name":"1.7","size_bytes":5,"download_url":"STORE/store/maps.apk","sha256":"HASH","install_source":"fdroid","rating_avg":4.5,"rating_count":120,"downloads":9000},{"package_name":"org.example.radio","label":"","version_code":1,"download_url":"STORE/store/radio.apk"}]}"#;

const ROWS: &str = r#"[{"program":"mathom","platform":"linux","arch":"x86_64","format":"deb","version_code":12,"version_name":"1.2.0","filename":"mathom_1.2.0.deb","size_bytes":5,"sha256":"HASH","label":"Mathom","uploaded_by":"ci","yanked":false,"created_at":"2026-10-28T12:00:00Z"}]"#;

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

#[quench_test::test]
fn signing_in_gives_a_session_and_bad_logins_say_why() {
    let (base, _) = start();
    let session = login(&base, "ann", "hunter2").unwrap();
    assert_eq!(session.refresh_token(), "refresh-1");
    assert_eq!(session.expires_in(), 900);
    assert_eq!(
        login(&base, "ann", "wrong").unwrap_err(),
        NetError::Unauthorized
    );
    assert_eq!(
        login(&base, "mfa", "hunter2").unwrap_err(),
        NetError::MfaRequired
    );
    assert!(
        login("http://127.0.0.1:1", "a", "b")
            .unwrap_err()
            .to_string()
            .contains("cannot reach")
    );
    for e in [
        NetError::Unauthorized,
        NetError::MfaRequired,
        NetError::RegistryDisabled,
        NetError::HashMismatch,
        NetError::Status(500),
        NetError::Network("x".into()),
    ] {
        assert!(!e.to_string().is_empty());
    }
}

#[quench_test::test]
fn the_catalog_is_read_and_an_expired_token_is_refreshed_once() {
    let (base, state) = start();
    let mut session = login(&base, "ann", "hunter2").unwrap();
    let rows = session.catalog("linux").unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        (
            rows[0].program.as_str(),
            rows[0].version_code,
            rows[0].title()
        ),
        ("mathom", 12, "Mathom")
    );
    // The server forgets the access token: a 401, then a refresh, then the request again.
    state.lock().unwrap().access = "something-else".into();
    let again = session.catalog("linux").unwrap();
    assert_eq!(again.len(), 1);
    assert_eq!(
        session.refresh_token(),
        "refresh-2",
        "the rotated token is what to keep"
    );
    let log = state.lock().unwrap().requests.clone();
    assert_eq!(log.iter().filter(|r| r.contains("auth/refresh")).count(), 1);
    // A session resumed from a stored refresh token starts by refreshing.
    let mut resumed = Session::resume(&format!("{base}/"), "refresh-2");
    assert_eq!(resumed.catalog("linux").unwrap().len(), 1);
    assert_eq!(resumed.refresh_token(), "refresh-3");
    // A refresh token that was already used is no good.
    let mut stale = Session::resume(&base, "refresh-2");
    assert_eq!(stale.catalog("linux").unwrap_err(), NetError::Unauthorized);
    // A server with no registry.
    state.lock().unwrap().catalog_status = 404;
    assert_eq!(
        resumed.catalog("linux").unwrap_err(),
        NetError::RegistryDisabled
    );
}

fn artifact(hash: &str, filename: &str) -> Artifact {
    let text = ROWS
        .replace("HASH", hash)
        .replace("mathom_1.2.0.deb", filename);
    quench_serde::json::from_str::<Vec<Artifact>>(&text)
        .unwrap()
        .remove(0)
}

#[quench_test::test]
fn a_download_is_checked_against_its_hash_before_it_is_kept() {
    let (base, _) = start();
    let mut session = login(&base, "ann", "hunter2").unwrap();
    let dir = std::env::temp_dir().join(format!("hoist-net-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut seen = Vec::new();
    let path = session
        .download(
            &artifact(&sha(b"hello"), "mathom_1.2.0.deb"),
            &dir,
            |done, total| seen.push((done, total)),
        )
        .unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"hello");
    assert_eq!(path, dir.join("mathom_1.2.0.deb"));
    assert_eq!(seen, [(5, 5)]);
    // A different file than the registry described: refused, and nothing is left behind.
    let bad = session.download(&artifact(&sha(b"other"), "evil.deb"), &dir, |_, _| {});
    assert_eq!(bad.unwrap_err(), NetError::HashMismatch);
    assert!(!dir.join("evil.deb").exists());
    // A name that tries to leave the directory is cut to its last part.
    let path = session
        .download(
            &artifact(&sha(b"hello"), "../../escape.deb"),
            &dir,
            |_, _| {},
        )
        .unwrap();
    assert_eq!(path, dir.join("escape.deb"));
    assert!(
        session
            .download(&artifact(&sha(b"hello"), ".."), &dir, |_, _| {})
            .is_err()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

fn public_app(base: &str, hash: &str, name: &str) -> hoist_core::PublicApp {
    let text = PAGE
        .replace("STORE", base.trim_end_matches('/'))
        .replace("HASH", hash);
    let mut app = quench_serde::json::from_str::<hoist_core::PublicPage>(&text)
        .unwrap()
        .items
        .remove(0);
    app.package_name = name.to_string();
    app
}

#[quench_test::test]
fn the_public_catalogue_is_browsed_searched_and_its_categories_listed() {
    let (base, state) = start();
    let mut session = login(&base, "ann", "hunter2").unwrap();
    let page = session
        .public_apps(&hoist_core::PublicQuery {
            query: Some(" map & co ".into()),
            category: Some("Navigation".into()),
            sort: "rating".into(),
            min_votes: Some(20),
            offset: 0,
            limit: 2,
        })
        .unwrap();
    assert_eq!(
        (page.total, page.items.len(), page.has_more()),
        (3, 2, true)
    );
    let maps = &page.items[0];
    assert_eq!(
        (maps.title(), maps.rating_text(), maps.is_verifiable()),
        ("Maps", "\u{2605} 4.5 (120)".to_string(), true)
    );
    assert_eq!(
        page.items[1].title(),
        "org.example.radio",
        "no label: the package"
    );
    assert_eq!(page.items[1].rating_text(), "");
    let asked = state.lock().unwrap().requests.last().cloned().unwrap();
    for part in [
        "sort=rating",
        "category=Navigation",
        "min_votes=20",
        "limit=2",
        "query=map%20%26%20co",
    ] {
        assert!(asked.contains(part), "{part} in {asked}");
    }
    let names: Vec<String> = session
        .public_categories()
        .unwrap()
        .into_iter()
        .map(|c| format!("{}:{}", c.name, c.count))
        .collect();
    assert_eq!(names, ["Games:40", "Internet:12"]);
    // Not served by this server.
    state.lock().unwrap().public_status = 404;
    assert_eq!(
        session
            .public_apps(&hoist_core::PublicQuery::default())
            .unwrap_err(),
        NetError::PublicDisabled
    );
    assert_eq!(
        session.public_categories().unwrap_err(),
        NetError::PublicDisabled
    );
}

#[quench_test::test]
fn a_public_download_carries_no_token_and_is_checked_when_it_can_be() {
    let (base, state) = start();
    let dir = std::env::temp_dir().join(format!("hoist-public-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let app = public_app(&base, &sha(b"hello"), "org.example.maps");
    let path = Session::download_public(&app, &dir, |_, _| {}).unwrap();
    assert_eq!(path, dir.join("org.example.maps-7.apk"));
    assert_eq!(std::fs::read(&path).unwrap(), b"hello");
    assert_eq!(
        state.lock().unwrap().store_auth,
        Some(false),
        "the store never sees the session"
    );
    // The wrong file is refused and not kept.
    let bad = public_app(&base, &sha(b"other"), "org.example.bad");
    assert_eq!(
        Session::download_public(&bad, &dir, |_, _| {}).unwrap_err(),
        NetError::HashMismatch
    );
    assert!(!dir.join("org.example.bad-7.apk").exists());
    // No checksum offered: kept, unverified.
    let mut open = public_app(&base, "", "org.example.open");
    open.sha256 = None;
    assert!(!open.is_verifiable());
    assert!(Session::download_public(&open, &dir, |_, _| {}).is_ok());
    // Plain http to somewhere else is not fetched; neither is a package name that is a path.
    let mut remote = public_app(&base, "", "org.example.remote");
    remote.download_url = "http://example.org/a.apk".into();
    assert!(Session::download_public(&remote, &dir, |_, _| {}).is_err());
    let mut sneaky = public_app(&base, "", "../escape");
    sneaky.sha256 = None;
    assert!(Session::download_public(&sneaky, &dir, |_, _| {}).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

quench_test::main!();
