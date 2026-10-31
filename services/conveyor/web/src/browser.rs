//! What a page uses: `fetch` with the session cookie, `EventSource` for a job's log, the page's storage and history.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use conveyor_app::{Conveyor, Msg, Request, Setup, conveyor};
use quench_core::time::Time;
use quench_ui::prelude::*;
use quench_ui::router::History;
use quench_ui_web::browser as env;
use quench_ui_web::{BrowserHistory, EventSource, SseEvent, sleep, spawn};

use crate::client::{Boxed, Client, HttpRequest, HttpResponse, Transport, base_of};

/// Where the app's `deliver` goes once the app exists.
type Inbox = Rc<RefCell<Option<Rc<dyn Fn(Msg)>>>>;

/// The job logs being read, by job.
type Streams = Rc<RefCell<HashMap<String, Rc<EventSource>>>>;

/// `fetch` to the service the page came from, with the cookie the sign-in set.
struct Fetch {
    base: String,
}

impl Transport for Fetch {
    fn send(&self, request: HttpRequest) -> Boxed<'_, Result<HttpResponse, String>> {
        let url = format!("{}{}", self.base, request.path);
        Box::pin(async move {
            let mut call = quench_ui_web::Request::new(request.method, &url)
                .header("Accept", "application/json");
            if let Some(body) = request.body {
                call = call.header("Content-Type", "application/json").body(body);
            }
            call.send()
                .await
                .map(|response| HttpResponse {
                    status: response.status,
                    body: response.text(),
                })
                .map_err(|e| e.to_string())
        })
    }
}

fn deliver(slot: &Inbox, msg: Msg) {
    let to = slot.borrow().clone();
    if let Some(to) = to {
        to(msg);
    }
}

/// How long lines are gathered before the screen is told. A finished job's log arrives all at once, thousands of lines in
/// the first moments; telling the screen per line redraws the whole list each time, which is quadratic in the length.
const LOG_BATCH_MS: u32 = 50;

/// Lines that arrived and were not yet handed to the screen, and whether a hand-over is already waiting on the timer.
#[derive(Default)]
struct Pending {
    lines: Vec<(bool, String)>,
    scheduled: bool,
}

/// Hands over what has gathered, as one message.
fn flush(slot: &Inbox, pending: &Rc<RefCell<Pending>>, job: &str) {
    let lines = {
        let mut p = pending.borrow_mut();
        p.scheduled = false;
        std::mem::take(&mut p.lines)
    };
    if !lines.is_empty() {
        deliver(
            slot,
            Msg::LogLines {
                job: job.to_string(),
                lines,
            },
        );
    }
}

/// Keeps `line` for the next hand-over, starting the timer for it when none is running.
fn gather(slot: &Inbox, pending: &Rc<RefCell<Pending>>, job: &str, stderr: bool, line: String) {
    let start = {
        let mut p = pending.borrow_mut();
        p.lines.push((stderr, line));
        !std::mem::replace(&mut p.scheduled, true)
    };
    if start {
        let (slot, pending, job) = (Rc::clone(slot), Rc::clone(pending), job.to_string());
        spawn(async move {
            sleep(LOG_BATCH_MS).await;
            flush(&slot, &pending, &job);
        });
    }
}

/// Reads a job's log as server-sent events: `stdout`/`stderr` lines, then `done` (which also ends the stream, so the browser
/// does not reconnect and read the log a second time).
fn stream_log(base: &str, job: String, slot: Inbox, streams: &Streams) {
    if streams.borrow().contains_key(&job) {
        return;
    }
    let url = format!("{base}/api/v1/jobs/{job}/stream");
    let source = Rc::new(EventSource::open(
        &url,
        &["stdout", "stderr", "done", "lagged"],
    ));
    streams.borrow_mut().insert(job.clone(), Rc::clone(&source));
    let streams = Rc::clone(streams);
    let pending: Rc<RefCell<Pending>> = Rc::default();
    spawn(async move {
        while let Some(event) = source.next().await {
            match event {
                SseEvent::Message { name, data } => match name.as_str() {
                    "done" => {
                        source.close();
                        break;
                    }
                    "lagged" => gather(&slot, &pending, &job, true, data),
                    other => gather(&slot, &pending, &job, other == "stderr", data),
                },
                SseEvent::Open | SseEvent::Reconnecting => {}
                SseEvent::Closed => break,
            }
        }
        streams.borrow_mut().remove(&job);
        // What is still waiting on the timer goes first, so the end never overtakes the last lines.
        flush(&slot, &pending, &job);
        deliver(&slot, Msg::LogDone { job });
    });
}

/// The app: the screens over `fetch`. The service is the one the page came from, and the cookie is the one the sign-in set.
#[must_use]
pub fn app() -> View {
    let history = BrowserHistory::new();
    let base = base_of(&history.location().path);
    let language = env::storage_get("language").unwrap_or_else(|| "en-US".to_string());
    let slot: Inbox = Rc::default();
    let streams: Streams = Rc::default();
    let client = Rc::new(Client::new(Fetch { base: base.clone() }));

    let requests: Rc<dyn Fn(Request)> = {
        let (slot, base) = (Rc::clone(&slot), base.clone());
        Rc::new(move |request| {
            match &request {
                Request::Language(code) => {
                    env::storage_set("language", Some(code));
                    return;
                }
                Request::OpenLog(job) => {
                    stream_log(&base, job.clone(), Rc::clone(&slot), &streams);
                    return;
                }
                Request::CloseLog(job) => {
                    let source = streams.borrow_mut().remove(job);
                    if let Some(source) = source {
                        source.close();
                    }
                    return;
                }
                _ => {}
            }
            let (slot, client) = (Rc::clone(&slot), Rc::clone(&client));
            spawn(async move {
                for msg in client.execute(request).await {
                    deliver(&slot, msg);
                }
            });
        })
    };
    let Conveyor {
        view,
        deliver: to_app,
    } = conveyor(
        requests,
        Setup {
            login_url: format!("{base}/ui/login"),
            base,
            language,
            history: Rc::new(history),
            now: Rc::new(|| Time::from_unix_millis(env::wall_ms())),
            home_refresh_ms: 5000,
            run_refresh_ms: 2000,
        },
    );
    *slot.borrow_mut() = Some(to_app);
    view
}

quench_ui_web::app!(app);
