//! What a page uses: `fetch` with the session cookie, `EventSource` for the answer of a turn, the page's storage and history.

use std::cell::RefCell;
use std::rc::Rc;

use quench_ui::prelude::*;
use quench_ui::router::History;
use quench_ui_web::browser as env;
use quench_ui_web::{BrowserHistory, EventSource, SseEvent, spawn};
use sage_app::{Msg, Request, Sage, Setup, sage};
use sage_core::StreamEvent;
use sage_core::stream::EVENT_NAMES;

use crate::client::{Boxed, Client, HttpRequest, HttpResponse, Transport, base_of};

/// Where the app's `deliver` goes once the app exists.
type Inbox = Rc<RefCell<Option<Rc<dyn Fn(Msg)>>>>;

/// The turn being read, if any.
type Stream = Rc<RefCell<Option<Rc<EventSource>>>>;

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

/// Stops reading the current turn's stream.
fn stop(stream: &Stream) {
    let source = stream.borrow_mut().take();
    if let Some(source) = source {
        source.close();
    }
}

/// Reads a turn's answer as server-sent events: status lines and pieces of the answer, then `done` or `failure` (either ends
/// the stream, so the browser does not reconnect and have the turn answered a second time).
fn stream_turn(base: &str, id: &str, slot: Inbox, stream: &Stream) {
    stop(stream);
    let url = format!("{base}/api/v1/turns/{id}/stream");
    let source = Rc::new(EventSource::open(&url, &EVENT_NAMES));
    *stream.borrow_mut() = Some(Rc::clone(&source));
    let stream = Rc::clone(stream);
    spawn(async move {
        let mut ended = false;
        while let Some(event) = source.next().await {
            match event {
                SseEvent::Message { name, data } => {
                    let Some(event) = StreamEvent::parse(&name, &data) else {
                        continue;
                    };
                    let last = matches!(event, StreamEvent::Done(_) | StreamEvent::Failure(_));
                    deliver(&slot, Msg::Stream(event));
                    if last {
                        ended = true;
                        source.close();
                        break;
                    }
                }
                SseEvent::Open | SseEvent::Reconnecting => {}
                SseEvent::Closed => break,
            }
        }
        // Another turn may have taken the place meanwhile: only clear the one that was read.
        let mine = stream
            .borrow()
            .as_ref()
            .is_some_and(|current| Rc::ptr_eq(current, &source));
        if mine {
            *stream.borrow_mut() = None;
            if !ended {
                deliver(&slot, Msg::StreamEnded);
            }
        }
    });
}

/// The app: the screens over `fetch`. The service is the one the page came from, and the cookie is the one the sign-in set.
#[must_use]
pub fn app() -> View {
    let history = BrowserHistory::new();
    let base = base_of(&history.location().path);
    let language = env::storage_get("language").unwrap_or_else(|| "en-US".to_string());
    let slot: Inbox = Rc::default();
    let stream: Stream = Rc::default();
    let client = Rc::new(Client::new(Fetch { base: base.clone() }));

    let requests: Rc<dyn Fn(Request)> = {
        let (slot, base) = (Rc::clone(&slot), base.clone());
        Rc::new(move |request| {
            match &request {
                Request::Language(code) => {
                    env::storage_set("language", Some(code));
                    return;
                }
                Request::StopStream => {
                    stop(&stream);
                    return;
                }
                _ => {}
            }
            let (slot, client, stream, base) = (
                Rc::clone(&slot),
                Rc::clone(&client),
                Rc::clone(&stream),
                base.clone(),
            );
            spawn(async move {
                for msg in client.execute(request).await {
                    // A turn that started is read as it is told to the app.
                    let started = if let Msg::TurnStarted(id) = &msg {
                        Some(id.clone())
                    } else {
                        None
                    };
                    deliver(&slot, msg);
                    if let Some(id) = started {
                        stream_turn(&base, &id, Rc::clone(&slot), &stream);
                    }
                }
            });
        })
    };
    let Sage {
        view,
        deliver: to_app,
    } = sage(
        requests,
        Setup {
            login_url: format!("{base}/ui/login"),
            base,
            language,
            history: Rc::new(history),
        },
    );
    *slot.borrow_mut() = Some(to_app);
    view
}

quench_ui_web::app!(app);
