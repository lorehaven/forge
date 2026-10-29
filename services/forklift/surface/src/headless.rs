//! A backup with no screen: the program the Android host's job service runs (`quench_surface --backup`). It scans,
//! sends what is waiting, and ends when there is nothing left to do (or the host says stop).
//!
//! The host does the device's side and speaks JSON lines. To the program's standard output the program writes
//! requests and reports; on its standard input it reads the answers and the events of uploads.
//!
//! Program to host: `{"id":N,"call":"media","kind":"images","offset":0}`, `{"id":N,"call":"folder","uri":...}`,
//! `{"id":N,"call":"upload","upload":"u1","uri":...,"url":...,"token":...}`, `{"id":N,"call":"device"}`,
//! `{"status":{"headline":...,"done":N,"waiting":N}}`, `{"notice":...}`, `{"signed_out":true}`, and last
//! `{"finished":true}`.
//!
//! Host to program: `{"id":N,"ok":...}` or `{"id":N,"err":"..."}`, `{"event":"done","id":"u1","status":200}`,
//! `{"event":"failed","id":"u1","message":...}`, and `{"event":"stop"}`.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use forklift_app::live::Msg;
use forklift_core::{Found, Remote};
use forklift_net::{Change, NetError, Storage, Store};
use quench_serde::{Map, Value, json};

use crate::worker::{Answer as WorkerAnswer, Shared, Worker, stored_session};
use crate::{Controller, Device, Node, Platform, Target, UploadEvent};

enum In {
    Line(Value),
    Token(u64, WorkerAnswer),
    Closed,
}

type Answer = Box<dyn FnOnce(Result<Value, String>)>;
type Timer = (Instant, Box<dyn FnOnce()>);

struct Stdio {
    out: Rc<RefCell<Box<dyn Write>>>,
    pending: RefCell<HashMap<u64, Answer>>,
    next: Cell<u64>,
    worker: Rc<Worker>,
    timers: RefCell<Vec<Timer>>,
}

fn object(pairs: Vec<(&str, Value)>) -> Value {
    let mut map = Map::new();
    for (k, v) in pairs {
        map.insert(k, v);
    }
    Value::Object(map)
}

fn text(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn write_line(out: &RefCell<Box<dyn Write>>, value: &Value) {
    let mut out = out.borrow_mut();
    let _ = writeln!(out, "{}", json::to_string(value));
    let _ = out.flush();
}

impl Stdio {
    fn call(&self, call: &str, mut fields: Vec<(&str, Value)>, then: Answer) {
        self.next.set(self.next.get() + 1);
        let id = self.next.get();
        self.pending.borrow_mut().insert(id, then);
        fields.push(("id", Value::from(i64::try_from(id).unwrap_or(0))));
        fields.push(("call", Value::String(call.into())));
        write_line(&self.out, &object(fields));
    }

    fn answered(&self, line: &Value) {
        let Some(id) = line.get("id").and_then(Value::as_u64) else {
            return;
        };
        let then = self.pending.borrow_mut().remove(&id);
        if let Some(then) = then {
            then(match line.get("err") {
                Some(e) => Err(e.as_str().unwrap_or("failed").to_string()),
                None => Ok(line.get("ok").cloned().unwrap_or_default()),
            });
        }
    }
}

fn found_of(row: &Value) -> Option<Found> {
    let uri = row.get("uri")?.as_str()?.to_string();
    Some(Found {
        local_id: uri.rsplit('/').next().unwrap_or(&uri).to_string(),
        name: text(row, "name"),
        size: row.get("size").and_then(Value::as_u64).unwrap_or(0),
        modified_ms: row.get("modified_ms").and_then(Value::as_i64).unwrap_or(0),
        uri,
    })
}

impl Platform for Stdio {
    fn media(
        &self,
        kind: &'static str,
        offset: u32,
        then: Box<dyn FnOnce(Result<Vec<Found>, String>)>,
    ) {
        self.call(
            "media",
            vec![
                ("kind", Value::String(kind.into())),
                ("offset", Value::from(i64::from(offset))),
            ],
            Box::new(move |answer| {
                then(answer.map(|rows| {
                    rows.as_array()
                        .map(|r| r.iter().filter_map(found_of).collect())
                        .unwrap_or_default()
                }));
            }),
        );
    }

    fn folder(&self, uri: &str, then: Box<dyn FnOnce(Result<Vec<Node>, String>)>) {
        self.call(
            "folder",
            vec![("uri", Value::String(uri.into()))],
            Box::new(move |answer| {
                then(answer.map(|rows| {
                    rows.as_array()
                        .map(|rows| {
                            rows.iter()
                                .filter_map(|row| {
                                    if matches!(row.get("is_dir"), Some(Value::Bool(true))) {
                                        Some(Node::Dir(text(row, "uri")))
                                    } else {
                                        found_of(row).map(Node::File)
                                    }
                                })
                                .collect()
                        })
                        .unwrap_or_default()
                }));
            }),
        );
    }

    fn upload(&self, id: &str, uri: &str, url: &str, token: &str) {
        // An answer is only an error when the host could not even start; the end of the upload is an event.
        self.call(
            "upload",
            vec![
                ("upload", Value::String(id.into())),
                ("uri", Value::String(uri.into())),
                ("url", Value::String(url.into())),
                ("token", Value::String(token.into())),
            ],
            Box::new(|_| {}),
        );
    }

    fn cancel(&self, _id: &str) {}

    fn changes(
        &self,
        storage: &str,
        since: i64,
        then: Box<dyn FnOnce(Result<Vec<Change>, NetError>)>,
    ) {
        self.worker.changes(storage, since, then);
    }

    fn listing(&self, storage: &str, then: Box<dyn FnOnce(Result<Vec<Remote>, NetError>)>) {
        self.worker.listing(storage, then);
    }

    fn quota(&self, storage: &str, then: Box<dyn FnOnce(Option<(u64, u64)>)>) {
        self.worker.quota(storage, then);
    }

    // A restore is the window's: a background run never starts one.
    fn download(&self, _id: &str, _url: &str, _token: &str, _name: &str, _target: &Target) {}

    fn cancel_download(&self, _id: &str) {}

    fn storages(&self, then: Box<dyn FnOnce(Result<Vec<Storage>, NetError>)>) {
        self.worker.storages(then);
    }

    fn token(&self, force: bool, then: Box<dyn FnOnce(Result<String, NetError>)>) {
        self.worker.token(force, then);
    }

    fn device(&self, then: Box<dyn FnOnce(Device)>) {
        self.call(
            "device",
            Vec::new(),
            Box::new(move |answer| {
                let state = answer.unwrap_or_default();
                let flag = |key: &str| matches!(state.get(key), Some(Value::Bool(true)));
                let transport = text(&state, "transport");
                then(Device {
                    online: flag("online"),
                    wifi: transport == "wifi" || transport == "ethernet",
                    charging: flag("charging"),
                });
            }),
        );
    }

    fn pick_folder(&self, then: Box<dyn FnOnce(Option<(String, String)>)>) {
        then(None);
    }

    fn now_ms(&self) -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
    }

    fn later(&self, ms: u64, f: Box<dyn FnOnce()>) {
        self.timers
            .borrow_mut()
            .push((Instant::now() + Duration::from_millis(ms), f));
    }
}

/// How a run ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ended {
    /// Nothing is left that can be done now.
    Idle,
    /// The host said stop (or closed the pipe).
    Stopped,
    /// There is no session, or the backup is switched off: nothing was done.
    Skipped(&'static str),
}

/// Runs one background backup over `input` and `output`.
///
/// # Errors
/// The streams'.
pub fn run(
    input: impl Read + Send + 'static,
    output: impl Write + 'static,
    store: &Store,
) -> io::Result<Ended> {
    let out: Rc<RefCell<Box<dyn Write>>> = Rc::new(RefCell::new(Box::new(output)));
    let finish = |why: Ended| {
        write_line(&out, &object(vec![("finished", Value::Bool(true))]));
        Ok(why)
    };
    let settings = store.settings();
    if !settings.auto {
        return finish(Ended::Skipped("automatic backup is off"));
    }
    if !settings.storage_confirmed {
        return finish(Ended::Skipped("no storage chosen yet"));
    }
    let session: Shared = std::sync::Arc::new(std::sync::Mutex::new(stored_session(
        store,
        &settings.server,
    )));
    if session.lock().unwrap().is_none() {
        return finish(Ended::Skipped("not signed in"));
    }

    let (tx, rx) = mpsc::channel::<In>();
    let reader_tx = tx.clone();
    std::thread::Builder::new()
        .name("backup-input".into())
        .spawn(move || {
            for line in BufReader::new(input).lines() {
                let Ok(line) = line else { break };
                if let Ok(value) = json::parse(&line)
                    && reader_tx.send(In::Line(value)).is_err()
                {
                    return;
                }
            }
            let _ = reader_tx.send(In::Closed);
        })?;
    let token_tx = std::sync::Mutex::new(tx);
    let worker = Worker::new(session, move |id, answer| {
        let _ = token_tx.lock().unwrap().send(In::Token(id, answer));
    });
    let platform = Rc::new(Stdio {
        out: Rc::clone(&out),
        pending: RefCell::default(),
        next: Cell::new(0),
        worker: Rc::clone(&worker),
        timers: RefCell::default(),
    });
    let emit_out = Rc::clone(&out);
    let controller = Controller::new(
        platform.clone(),
        store.clone(),
        Rc::new(move |m| match m {
            Msg::Snapshot(s) => write_line(
                &emit_out,
                &object(vec![(
                    "status",
                    object(vec![
                        ("headline", Value::String(s.headline())),
                        (
                            "done",
                            Value::from(i64::try_from(s.totals.done).unwrap_or(0)),
                        ),
                        (
                            "waiting",
                            Value::from(i64::try_from(s.totals.waiting()).unwrap_or(0)),
                        ),
                    ]),
                )]),
            ),
            Msg::Notice(t) => write_line(&emit_out, &object(vec![("notice", Value::String(t))])),
            Msg::SignedOut => {
                write_line(&emit_out, &object(vec![("signed_out", Value::Bool(true))]))
            }
            _ => {}
        }),
    );
    controller.scan(false);

    let ended = loop {
        if controller.is_idle() {
            break Ended::Idle;
        }
        // Timers that are due run first; then wait for the host for no longer than the next one.
        let due: Vec<Box<dyn FnOnce()>> = {
            let mut timers = platform.timers.borrow_mut();
            let now = Instant::now();
            let (ready, later): (Vec<_>, Vec<_>) = timers.drain(..).partition(|(at, _)| *at <= now);
            *timers = later;
            ready.into_iter().map(|(_, f)| f).collect()
        };
        for f in due {
            f();
        }
        let wait = platform
            .timers
            .borrow()
            .iter()
            .map(|(at, _)| at.saturating_duration_since(Instant::now()))
            .min()
            .unwrap_or(Duration::from_secs(1))
            .min(Duration::from_secs(1));
        match rx.recv_timeout(wait) {
            Ok(In::Line(line)) => match line.get("event").and_then(Value::as_str) {
                Some("done") => controller.upload_event(&UploadEvent::Done {
                    id: text(&line, "id"),
                    status: line
                        .get("status")
                        .and_then(Value::as_u64)
                        .and_then(|s| u16::try_from(s).ok())
                        .unwrap_or(0),
                }),
                Some("failed") => controller.upload_event(&UploadEvent::Failed {
                    id: text(&line, "id"),
                    message: text(&line, "message"),
                }),
                Some("stop") => break Ended::Stopped,
                _ => platform.answered(&line),
            },
            Ok(In::Token(id, answer)) => worker.complete(id, answer),
            Ok(In::Closed) | Err(mpsc::RecvTimeoutError::Disconnected) => break Ended::Stopped,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
    };
    // Whatever was in flight waits again, and the manifest is on disk.
    controller.sign_out_keep_session();
    finish(ended)
}
