//! What a page uses: `fetch` with the session cookie, the page's storage, the browser's clock and history.

use std::cell::RefCell;
use std::rc::Rc;

use gauge_app::{Gauge, Msg, Request, Setup, gauge};
use quench_ui::prelude::*;
use quench_ui::router::History;
use quench_ui_web::browser as env;
use quench_ui_web::{BrowserHistory, spawn};

use crate::protocol::{base_of, interpret, unreachable, url_for};

/// Where the app's `deliver` goes once the app exists.
type Inbox = Rc<RefCell<Option<Rc<dyn Fn(Msg)>>>>;

/// How often the open screen asks again.
const REFRESH_MS: u32 = 10_000;

#[allow(clippy::cast_precision_loss)] // milliseconds since 1970 are far below 2^53
fn now() -> f64 {
    env::wall_ms() as f64 / 1000.0
}

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
            let Some(url) = url_for(&base, &request) else {
                if let Request::Language(code) = &request {
                    env::storage_set("language", Some(code));
                }
                return;
            };
            let slot = Rc::clone(&slot);
            spawn(async move {
                let msg = match quench_ui_web::Request::get(&url)
                    .header("Accept", "application/json")
                    .send()
                    .await
                {
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
    let Gauge { view, deliver } = gauge(
        requests,
        Setup {
            login_url: format!("{base}/ui/login"),
            base,
            language,
            history: Rc::new(history),
            now: Rc::new(now),
            refresh_ms: REFRESH_MS,
        },
    );
    *slot.borrow_mut() = Some(deliver);
    view
}

quench_ui_web::app!(app);
