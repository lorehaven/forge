//! What a page uses: `fetch` with the session cookie, the page's storage, history and clock.

use std::cell::RefCell;
use std::rc::Rc;

use mailroom_app::{Mailroom, Msg, Request, Setup, mailroom};
use quench_core::time::Time;
use quench_ui::prelude::*;
use quench_ui::router::History;
use quench_ui_web::browser as env;
use quench_ui_web::{BrowserHistory, spawn};

use crate::client::{Boxed, Client, HttpRequest, HttpResponse, Transport, base_of};

/// Where the app's `deliver` goes once the app exists.
type Inbox = Rc<RefCell<Option<Rc<dyn Fn(Msg)>>>>;

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

/// How far the browser's zone is east of UTC now, in minutes (0 when the zone is not known).
fn utc_offset_minutes(now_seconds: i64) -> i32 {
    let zone = env::time_zone();
    quench_tz::Zone::named(&zone).map_or(0, |z| {
        z.offset_at(Time::from_unix(now_seconds)).utc_offset / 60
    })
}

/// The app: the screens over `fetch`. The service is the one the page came from, and the cookie is the one the sign-in set.
#[must_use]
pub fn app() -> View {
    let history = BrowserHistory::new();
    let base = base_of(&history.location().path);
    let language = env::storage_get("language").unwrap_or_else(|| "en-US".to_string());
    let slot: Inbox = Rc::default();
    let client = Rc::new(Client::new(Fetch { base: base.clone() }));
    let now = env::wall_ms() / 1000;

    let requests: Rc<dyn Fn(Request)> = {
        let slot = Rc::clone(&slot);
        Rc::new(move |request| {
            if let Request::Language(code) = &request {
                env::storage_set("language", Some(code));
                return;
            }
            let (slot, client) = (Rc::clone(&slot), Rc::clone(&client));
            spawn(async move {
                for msg in client.execute(request).await {
                    let deliver = slot.borrow().clone();
                    if let Some(deliver) = deliver {
                        deliver(msg);
                    }
                }
            });
        })
    };
    let Mailroom { view, deliver } = mailroom(
        requests,
        Setup {
            login_url: format!("{base}/ui/login"),
            base,
            language,
            utc_offset_minutes: utc_offset_minutes(now),
            now,
            history: Rc::new(history),
        },
    );
    *slot.borrow_mut() = Some(deliver);
    view
}

quench_ui_web::app!(app);
