//! The background run against a fake host (the job service's stand-in) and a stand-in gatehouse.
#![allow(clippy::unwrap_used, clippy::type_complexity)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use forklift_core::{Settings, Status};
use forklift_net::Store;
use forklift_surface::headless::{Ended, run};
use quench_serde::{Value, json};

/// A gatehouse that answers every call with fresh numbered tokens.
fn gatehouse() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let issued = Arc::new(Mutex::new(0u32));
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let issued = Arc::clone(&issued);
            std::thread::spawn(move || {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut length = 0usize;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line.trim().is_empty() {
                        break;
                    }
                    if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = v.trim().parse().unwrap();
                    }
                }
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                let n = {
                    let mut n = issued.lock().unwrap();
                    *n += 1;
                    *n
                };
                let json = format!(
                    "{{\"access_token\":\"access-{n}\",\"refresh_token\":\"refresh-{n}\",\"token_type\":\"Bearer\",\"expires_in\":900}}"
                );
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{json}",
                    json.len()
                );
            });
        }
    });
    format!("http://{addr}")
}

/// What the fake host does with what the program asks.
struct Host {
    /// Everything the program wrote.
    lines: Arc<Mutex<Vec<Value>>>,
    stop: std::sync::mpsc::Sender<()>,
}

/// Starts a host answering `answer(call line)` with the lines to write back. Returns it with the pipe ends for `run`.
fn host(
    answer: impl Fn(&Value) -> Vec<String> + Send + 'static,
) -> (Host, std::io::PipeReader, std::io::PipeWriter) {
    let (to_program_r, mut to_program_w) = std::io::pipe().unwrap();
    let (from_program_r, from_program_w) = std::io::pipe().unwrap();
    let lines: Arc<Mutex<Vec<Value>>> = Arc::default();
    let seen = Arc::clone(&lines);
    let (stop, stopped) = std::sync::mpsc::channel::<()>();
    std::thread::spawn(move || {
        for line in BufReader::new(from_program_r).lines() {
            let Ok(line) = line else { break };
            let value = json::parse(&line).unwrap();
            seen.lock().unwrap().push(value.clone());
            for reply in answer(&value) {
                let _ = writeln!(to_program_w, "{reply}");
            }
            if stopped.try_recv().is_ok() {
                let _ = writeln!(to_program_w, "{{\"event\":\"stop\"}}");
            }
        }
    });
    (Host { lines, stop }, to_program_r, from_program_w)
}

fn id(call: &Value) -> u64 {
    call.get("id").and_then(Value::as_u64).unwrap()
}

fn media_row(i: u32) -> String {
    format!(
        "{{\"uri\":\"content://media/external/images/media/{i}\",\"name\":\"IMG_{i}.jpg\",\"size\":10,\"modified_ms\":{}}}",
        i * 1000
    )
}

fn dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("forklift-headless-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn store(name: &str, server: &str, auto: bool, token: Option<&str>) -> Store {
    let store = Store::new(dir(name));
    store.set_settings(&Settings {
        server: server.into(),
        storage: "moto".into(),
        storage_confirmed: true,
        auto,
        videos: false,
        ..Settings::default()
    });
    if let Some(t) = token {
        store.set_token(t);
    }
    store
}

/// The usual host: three photos, a good network, every upload succeeds.
fn happy(online: bool) -> impl Fn(&Value) -> Vec<String> + Send + 'static {
    move |call| {
        let Some(name) = call.get("call").and_then(Value::as_str) else {
            return vec![];
        };
        let n = id(call);
        match name {
            "media" if call.get("offset").and_then(Value::as_i64) == Some(0) => {
                vec![format!(
                    "{{\"id\":{n},\"ok\":[{}]}}",
                    (1..=3).map(media_row).collect::<Vec<_>>().join(",")
                )]
            }
            "media" => vec![format!("{{\"id\":{n},\"ok\":[]}}")],
            "device" => vec![format!(
                "{{\"id\":{n},\"ok\":{{\"online\":{online},\"transport\":\"wifi\",\"charging\":false}}}}"
            )],
            "upload" => {
                let up = call.get("upload").and_then(Value::as_str).unwrap();
                vec![
                    format!("{{\"id\":{n},\"ok\":{{}}}}"),
                    format!("{{\"event\":\"done\",\"id\":\"{up}\",\"status\":200}}"),
                ]
            }
            _ => vec![],
        }
    }
}

#[quench_test::test]
fn a_run_scans_sends_everything_and_ends_when_there_is_nothing_left() {
    let server = gatehouse();
    let store = store("full", &server, true, Some("t0"));
    let (host, input, output) = host(happy(true));
    let ended = run(input, output, &store).unwrap();
    assert_eq!(ended, Ended::Idle);
    let manifest = store.manifest();
    assert_eq!(
        manifest
            .items()
            .filter(|i| i.status == Status::Done)
            .count(),
        3
    );
    let lines = host.lines.lock().unwrap().clone();
    let uploads: Vec<&Value> = lines
        .iter()
        .filter(|l| l.get("call").and_then(Value::as_str) == Some("upload"))
        .collect();
    assert_eq!(uploads.len(), 3);
    // The token is the one the stand-in gatehouse gave, and the URL goes to the storage.
    assert_eq!(
        uploads[0].get("token").and_then(Value::as_str),
        Some("access-1")
    );
    assert!(
        uploads[0]
            .get("url")
            .and_then(Value::as_str)
            .unwrap()
            .starts_with(&format!(
                "{server}/warehouse/api/v1/files/moto/file?path=photos"
            ))
    );
    let last_status = lines.iter().rev().find_map(|l| l.get("status")).unwrap();
    assert_eq!(
        last_status.get("headline").and_then(Value::as_str),
        Some("All 3 files are backed up")
    );
    assert_eq!(
        lines.last().unwrap().get("finished"),
        Some(&Value::Bool(true))
    );
    // The rotated refresh token was kept for the next run.
    assert_eq!(store.token().as_deref(), Some("refresh-1"));
    let _ = std::fs::remove_dir_all(store.dir());
}

#[quench_test::test]
fn nothing_is_asked_when_the_backup_is_off_or_nobody_is_signed_in() {
    for (name, auto, token, why) in [
        ("off", false, Some("t"), "off"),
        ("nobody", true, None, "not signed in"),
    ] {
        let (host, input, output) = host(happy(true));
        let store = store(name, "http://127.0.0.1:1", auto, token);
        let ended = run(input, output, &store).unwrap();
        assert!(
            matches!(ended, Ended::Skipped(w) if w.contains(why)),
            "{ended:?}"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
        let lines = host.lines.lock().unwrap().clone();
        assert_eq!(lines.len(), 1, "only `finished`: {lines:?}");
        let _ = std::fs::remove_dir_all(store.dir());
    }
}

#[quench_test::test]
fn without_a_network_the_run_scans_and_ends_having_sent_nothing() {
    let server = gatehouse();
    let store = store("offline", &server, true, Some("t0"));
    let (host, input, output) = host(happy(false));
    assert_eq!(run(input, output, &store).unwrap(), Ended::Idle);
    assert!(
        host.lines
            .lock()
            .unwrap()
            .iter()
            .all(|l| l.get("call").and_then(Value::as_str) != Some("upload"))
    );
    assert_eq!(
        store.manifest().len(),
        3,
        "the files are known, waiting for a network"
    );
    let _ = std::fs::remove_dir_all(store.dir());
}

#[quench_test::test]
fn the_host_can_stop_a_run_and_what_was_sending_waits_again() {
    let server = gatehouse();
    let store = store("stop", &server, true, Some("t0"));
    let tell = std::sync::Arc::new(Mutex::new(None::<std::sync::mpsc::Sender<()>>));
    let answers = happy(true);
    let trigger = Arc::clone(&tell);
    // Uploads are started but never finished; the first of them makes the host say stop.
    let (host, input, output) = host(move |call| {
        if call.get("call").and_then(Value::as_str) == Some("upload") {
            if let Some(stop) = trigger.lock().unwrap().as_ref() {
                let _ = stop.send(());
            }
            let n = id(call);
            return vec![format!("{{\"id\":{n},\"ok\":{{}}}}")];
        }
        answers(call)
    });
    *tell.lock().unwrap() = Some(host.stop.clone());
    let ended = run(input, output, &store).unwrap();
    assert_eq!(ended, Ended::Stopped);
    let manifest = store.manifest();
    assert!(
        manifest.items().all(|i| i.status == Status::Pending),
        "in-flight files wait again"
    );
    let _ = std::fs::remove_dir_all(store.dir());
}

#[quench_test::test]
fn a_run_does_nothing_until_a_storage_has_been_chosen() {
    let store = store("unchosen", "http://127.0.0.1:1", true, Some("t"));
    let mut settings = store.settings();
    settings.storage_confirmed = false;
    store.set_settings(&settings);
    let (host, input, output) = host(happy(true));
    let ended = run(input, output, &store).unwrap();
    assert!(
        matches!(ended, Ended::Skipped(w) if w.contains("no storage")),
        "{ended:?}"
    );
    std::thread::sleep(std::time::Duration::from_millis(50));
    assert_eq!(host.lines.lock().unwrap().len(), 1, "only `finished`");
    let _ = std::fs::remove_dir_all(store.dir());
}

quench_test::main!();
