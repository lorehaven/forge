// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::unwrap_used, clippy::too_many_lines)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

use daybook_core::ListRef;
use daybook_net::{NetError, Settings, Store, login};
use quench_serde::Value;

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

/// A gatehouse that hands out numbered tokens, and a daybook that answers with `api`.
fn stand_in(
    api: impl Fn(&Seen) -> (u16, Vec<(String, String)>, String) + Send + Sync + 'static,
) -> (String, Arc<Mutex<Vec<Seen>>>) {
    let issued = Arc::new(Mutex::new(0u32));
    serve(Box::new(move |seen| {
        if seen.target.starts_with("/gatehouse/api/v1/auth/") {
            let mut n = issued.lock().unwrap();
            *n += 1;
            let ok = seen.target.ends_with("login")
                && String::from_utf8_lossy(&seen.body).contains("secret")
                || seen.target.ends_with("refresh");
            return if ok {
                (200, vec![], tokens(*n))
            } else {
                (401, vec![], "{}".into())
            };
        }
        api(seen)
    }))
}

fn tokens(n: u32) -> String {
    format!(
        "{{\"access_token\":\"access-{n}\",\"refresh_token\":\"refresh-{n}\",\"token_type\":\"Bearer\",\"expires_in\":900}}"
    )
}

const TASK: &str = r#"{"id":"t1","project_id":null,"content":"Milk","is_completed":false,"sort_order":0,"due_at":null,"due_all_day":false,"label_ids":[],"created_at":"2026-10-01T00:00:00Z","updated_at":"2026-10-01T00:00:00Z"}"#;

#[quench_test::test]
fn open_tasks_are_asked_for_across_all_lists() {
    let (base, log) = stand_in(|_| (200, vec![], format!("[{TASK}]")));
    let mut session = login(&base, "me", "secret").unwrap();
    let tasks = session.open_tasks().unwrap();
    assert_eq!(tasks.len(), 1);
    let seen = log.lock().unwrap();
    let get = seen
        .iter()
        .find(|s| s.target.starts_with("/daybook/"))
        .unwrap();
    assert_eq!(get.target, "/daybook/api/v1/tasks?all=true&completed=false");
    assert!(get.auth.starts_with("Bearer access-"));
}

#[quench_test::test]
fn a_wrong_password_is_unauthorized() {
    let (base, _) = stand_in(|_| (200, vec![], "[]".into()));
    assert_eq!(
        login(&base, "me", "nope").unwrap_err(),
        NetError::Unauthorized
    );
}

#[quench_test::test]
fn an_update_replaces_the_task_with_json() {
    let (base, log) = stand_in(|_| (200, vec![], TASK.into()));
    let mut session = login(&base, "me", "secret").unwrap();
    let mut body = quench_serde::Map::new();
    body.insert("content", Value::String("Milk".into()));
    let task = session.update_task("t 1", &Value::Object(body)).unwrap();
    assert_eq!(task.id, "t1");
    let seen = log.lock().unwrap();
    let put = seen.iter().find(|s| s.method == "PUT").unwrap();
    assert_eq!(put.target, "/daybook/api/v1/tasks/t%201");
    assert!(put.content_type.contains("json"));
    assert!(String::from_utf8_lossy(&put.body).contains("\"content\""));
}

#[quench_test::test]
fn a_token_that_expired_is_renewed_once() {
    let calls = Arc::new(Mutex::new(0));
    let c = Arc::clone(&calls);
    let (base, _) = stand_in(move |seen| {
        let mut n = c.lock().unwrap();
        *n += 1;
        if seen.auth == "Bearer access-1" {
            (401, vec![], "{}".into())
        } else {
            (200, vec![], "[]".into())
        }
    });
    let mut session = login(&base, "me", "secret").unwrap();
    assert!(session.projects().unwrap().is_empty());
    assert_eq!(*calls.lock().unwrap(), 2);
}

#[quench_test::test]
fn statuses_become_errors_a_screen_can_say() {
    for (status, error) in [
        (409, NetError::NameTaken),
        (404, NetError::NotFound),
        (400, NetError::Invalid),
        (403, NetError::Forbidden),
        (500, NetError::Status(500)),
    ] {
        let (base, _) = stand_in(move |_| (status, vec![], "{}".into()));
        let mut session = login(&base, "me", "secret").unwrap();
        assert_eq!(session.create_label("x", "#112233").unwrap_err(), error);
    }
}

#[quench_test::test]
fn a_delete_with_no_body_is_fine() {
    let (base, log) = stand_in(|_| (204, vec![], String::new()));
    let mut session = login(&base, "me", "secret").unwrap();
    session.delete_task("t1").unwrap();
    session.delete_project("p1").unwrap();
    session.delete_label("l1").unwrap();
    let seen = log.lock().unwrap();
    assert_eq!(seen.iter().filter(|s| s.method == "DELETE").count(), 3);
}

#[quench_test::test]
fn settings_and_the_session_survive_a_restart() {
    let dir = std::env::temp_dir().join(format!("daybook-net-{}", std::process::id()));
    let store = Store::new(&dir);
    assert_eq!(store.settings(), Settings::default());
    let settings = Settings {
        server: "https://x".into(),
        user: "me".into(),
        list: ListRef::Project("p".into()),
    };
    store.set_settings(&settings);
    store.set_token("refresh-9");
    let again = Store::new(&dir);
    assert_eq!(again.settings(), settings);
    assert_eq!(again.token().as_deref(), Some("refresh-9"));
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(dir.join("session"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o077, 0, "the session is private");
    }
    again.clear_token();
    assert_eq!(again.token(), None);
    let _ = std::fs::remove_dir_all(&dir);
}

quench_test::main!();
