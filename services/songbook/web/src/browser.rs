//! What a page uses: `fetch` with the session cookie, the page's storage and history.

use std::cell::RefCell;
use std::rc::Rc;

use quench_ui::prelude::*;
use quench_ui::router::History;
use quench_ui_web::browser as env;
use quench_ui_web::{BrowserHistory, spawn};
use songbook_app::{Msg, Request, Setup, Songbook, songbook};

use crate::protocol::{base_of, call_for, interpret, unreachable};

/// Where the app's `deliver` goes once the app exists.
type Inbox = Rc<RefCell<Option<Rc<dyn Fn(Msg)>>>>;

/// How often the job is asked about while it runs, and otherwise (for the pill in the top row).
const BUSY_MS: u32 = 1_500;
const IDLE_MS: u32 = 5_000;

/// The app: the screens over `fetch`. The service is the one the page came from, and the cookie is the one the sign-in set.
#[must_use]
pub fn app() -> View {
    let history = BrowserHistory::new();
    let base = base_of(&history.location().path);
    let language = env::storage_get("language").unwrap_or_else(|| "en-US".to_string());
    let slot: Inbox = Rc::default();

    let requests: Rc<dyn Fn(Request)> = {
        let (slot, base) = (Rc::clone(&slot), base.clone());
        Rc::new(move |request| {
            let Some(call) = call_for(&request) else {
                if let Request::Language(code) = &request {
                    env::storage_set("language", Some(code));
                }
                return;
            };
            let (slot, url) = (Rc::clone(&slot), format!("{base}{}", call.path));
            spawn(async move {
                let mut http = quench_ui_web::Request::new(call.method, &url)
                    .header("Accept", "application/json");
                if let Some(body) = call.body {
                    http = http.header("Content-Type", "application/json").body(body);
                }
                let msg = match http.send().await {
                    Ok(response) => interpret(&request, response.status, &response.text()),
                    Err(e) => unreachable(&e.to_string()),
                };
                let deliver = slot.borrow().clone();
                if let Some(deliver) = deliver {
                    deliver(msg);
                }
            });
        })
    };
    let Songbook { view, deliver } = songbook(
        requests,
        Setup {
            login_url: format!("{base}/ui/login"),
            base,
            language,
            history: Rc::new(history),
            busy_ms: BUSY_MS,
            idle_ms: IDLE_MS,
        },
    );
    *slot.borrow_mut() = Some(deliver);
    view
}

quench_ui_web::app!(app);
