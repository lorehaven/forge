// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::unwrap_used, clippy::too_many_lines)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

use gatehouse_client::{Error, Session, login};

/// What a stand-in server saw of one request.
#[derive(Clone, Debug, Default)]
struct Seen {
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
        target: {
            parts.next(); // the method
            parts.next().unwrap().into()
        },
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

/// A gatehouse that numbers its tokens (and accepts any refresh token that is not `spent`), and answers everything
/// else with `rest`.
fn gatehouse(
    spent: &'static [&'static str],
    rest: impl Fn(&Seen) -> u16 + Send + Sync + 'static,
) -> (String, Arc<Mutex<Vec<Seen>>>) {
    let issued = Arc::new(Mutex::new(0u32));
    serve(Box::new(move |seen| {
        if seen.target == "/gatehouse/api/v1/auth/refresh" {
            let body = String::from_utf8_lossy(&seen.body).to_string();
            if spent.iter().any(|s| body.contains(&format!("\"{s}\""))) {
                return (401, vec![], String::new());
            }
        }
        if seen.target.starts_with("/gatehouse/api/v1/auth/") {
            let mut n = issued.lock().unwrap();
            *n += 1;
            return (200, vec![], tokens(*n, 900));
        }
        (rest(seen), vec![], String::new())
    }))
}

fn get(session: &mut Session, base: &str, path: &str) -> u16 {
    let url = format!("{base}{path}");
    session
        .call("GET", path, false, |c, token| {
            c.get(&url).bearer_auth(token).send()
        })
        .unwrap()
        .status()
        .as_u16()
}

#[quench_test::test]
fn bad_logins_say_why() {
    for (body, expected) in [
        (r#"{"error":"mfa_required"}"#, Error::MfaRequired),
        (r#"{"error":"account_locked"}"#, Error::AccountLocked),
        (r#"{"error":"account_disabled"}"#, Error::AccountDisabled),
        (r#"{"error":"email_unverified"}"#, Error::EmailUnverified),
        (r#"{"error":"nope"}"#, Error::Unauthorized),
    ] {
        let (base, _) = serve(Box::new(move |_| (401, vec![], body.into())));
        assert_eq!(login(&base, "u", "p").unwrap_err(), expected, "{body}");
    }
    let (base, _) = serve(Box::new(|_| (500, vec![], String::new())));
    assert_eq!(login(&base, "u", "p").unwrap_err(), Error::Status(500));
}

#[quench_test::test]
fn the_password_goes_once_and_the_session_does_not_print_its_tokens() {
    let (base, log) = gatehouse(&[], |_| 200);
    let session = login(&format!("{base}/"), "ann", "hunter2").unwrap();
    let seen = log.lock().unwrap()[0].clone();
    assert_eq!(seen.target, "/gatehouse/api/v1/auth/login");
    assert!(String::from_utf8_lossy(&seen.body).contains("hunter2"));
    assert_eq!(session.base(), base);
    let shown = format!("{session:?}");
    assert!(
        !shown.contains("access-1") && !shown.contains("refresh-1"),
        "{shown}"
    );
}

#[quench_test::test]
fn a_token_with_life_left_is_not_refreshed() {
    let (base, log) = gatehouse(&[], |_| 200);
    let mut s = login(&base, "u", "p").unwrap();
    assert_eq!(s.fresh_token().unwrap(), "access-1");
    assert_eq!(log.lock().unwrap().len(), 1);
}

#[quench_test::test]
fn a_token_at_the_end_of_its_life_is_renewed_and_the_new_refresh_token_is_kept_at_once() {
    let issued = Arc::new(Mutex::new(0u32));
    let counter = Arc::clone(&issued);
    let (base, _) = serve(Box::new(move |_| {
        let mut n = counter.lock().unwrap();
        *n += 1;
        (200, vec![], tokens(*n, 0)) // born stale
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
fn a_new_refresh_token_is_kept_even_when_the_request_that_needed_it_fails() {
    let (base, _) = gatehouse(&[], |_| 500);
    let kept = Arc::new(Mutex::new(Vec::<String>::new()));
    let sink = Arc::clone(&kept);
    let mut s = Session::resume(&base, "old")
        .keeping_tokens(move |t| sink.lock().unwrap().push(t.to_string()));
    assert_eq!(get(&mut s, &base, "/x"), 500);
    assert_eq!(*kept.lock().unwrap(), ["refresh-1"]);
}

#[quench_test::test]
fn a_refused_token_is_renewed_once_and_the_request_made_again() {
    let first = Arc::new(Mutex::new(true));
    let (base, log) = gatehouse(&[], move |_| {
        if std::mem::take(&mut *first.lock().unwrap()) {
            401
        } else {
            200
        }
    });
    let mut s = Session::resume(&base, "r");
    assert_eq!(get(&mut s, &base, "/x"), 200);
    let auths: Vec<String> = log
        .lock()
        .unwrap()
        .iter()
        .filter(|s| s.target == "/x")
        .map(|s| s.auth.clone())
        .collect();
    assert_eq!(auths, ["Bearer access-1", "Bearer access-2"]);
}

#[quench_test::test]
fn a_request_refused_twice_is_the_servers_answer_not_a_loop() {
    let (base, log) = gatehouse(&[], |_| 401);
    let mut s = Session::resume(&base, "r");
    assert_eq!(get(&mut s, &base, "/x"), 401);
    assert_eq!(
        log.lock()
            .unwrap()
            .iter()
            .filter(|s| s.target == "/x")
            .count(),
        2
    );
}

#[quench_test::test]
fn a_token_another_program_kept_is_used_before_a_refresh_and_after_a_refusal() {
    // Before: the other program already rotated "mine", so its newer token is the one to send.
    let (base, log) = gatehouse(&["mine"], |_| 200);
    let mut s = Session::resume(&base, "mine").following(|| Some("theirs".to_string()));
    s.refresh().unwrap();
    assert!(String::from_utf8_lossy(&log.lock().unwrap()[0].body).contains("theirs"));
    // After: the check made the refresh with "mine" just before the other kept "theirs".
    let kept = Arc::new(Mutex::new(vec!["mine".to_string()]));
    let reads = Arc::clone(&kept);
    let (base, log) = gatehouse(&["mine"], |_| 200);
    let mut s = Session::resume(&base, "mine").following(move || reads.lock().unwrap().pop());
    kept.lock().unwrap().insert(0, "theirs".to_string());
    s.refresh().unwrap();
    let bodies: Vec<String> = log
        .lock()
        .unwrap()
        .iter()
        .map(|s| String::from_utf8_lossy(&s.body).to_string())
        .collect();
    assert!(bodies.last().unwrap().contains("theirs"), "{bodies:?}");
}

#[quench_test::test]
fn a_refused_refresh_leaves_the_hooks_in_place() {
    let refused = Arc::new(Mutex::new(true));
    let (base, _) = serve(Box::new(move |_| {
        if std::mem::take(&mut *refused.lock().unwrap()) {
            (401, vec![], String::new())
        } else {
            (200, vec![], tokens(7, 900))
        }
    }));
    let kept = Arc::new(Mutex::new(Vec::<String>::new()));
    let sink = Arc::clone(&kept);
    let mut s = Session::resume(&base, "r")
        .keeping_tokens(move |t| sink.lock().unwrap().push(t.to_string()));
    assert_eq!(s.refresh().unwrap_err(), Error::Unauthorized);
    s.refresh().unwrap();
    assert_eq!(
        *kept.lock().unwrap(),
        ["refresh-7"],
        "the hook still works after a refusal"
    );
}

#[quench_test::test]
fn a_server_that_is_not_there_is_a_network_error() {
    let mut s = Session::resume("http://127.0.0.1:1", "r");
    assert!(matches!(s.refresh().unwrap_err(), Error::Network(_)));
}

quench_test::main!();
