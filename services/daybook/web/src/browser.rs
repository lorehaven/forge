//! What a page uses: `fetch`, the page's storage, the browser's clock and zone, and the executor.

use std::rc::Rc;

use daybook_app::{Daybook, Setup, daybook};
use daybook_core::{ListRef, Request};
use daybook_host::Controller;
use daybook_net::Store;
use quench_core::time::Time;
use quench_tz::Zone;
use quench_ui::prelude::*;
use quench_ui_web::browser as env;
use quench_ui_web::{Request as Fetching, spawn};

use crate::client::{Boxed, Client, HttpRequest, HttpResponse, Keep, Transport};
use crate::platform::{Later, Spawn, Web};

/// Sends a request with `fetch`, to the server the page came from.
pub struct Fetch;

impl Transport for Fetch {
    fn send(&self, request: HttpRequest) -> Boxed<'_, Result<HttpResponse, String>> {
        Box::pin(async move {
            let mut call = Fetching::new(request.method, &request.path);
            if let Some(token) = &request.bearer {
                call = call.header("Authorization", &format!("Bearer {token}"));
            }
            if let Some(body) = request.body {
                call = call
                    .header("Content-Type", "application/json")
                    .body(body.into_bytes());
            }
            match call.send().await {
                Ok(response) => Ok(HttpResponse {
                    status: response.status,
                    body: response.text(),
                }),
                Err(e) => Err(e.to_string()),
            }
        })
    }
}

/// The refresh token, in the page's storage.
pub struct Stored;

impl Keep for Stored {
    fn load(&self) -> Option<String> {
        env::storage_get("refresh")
    }

    fn save(&self, token: Option<&str>) {
        env::storage_set("refresh", token);
    }
}

/// The platform of this page, with the clock and the zone of the browser.
#[must_use]
pub fn platform() -> Rc<Web<Fetch, Stored>> {
    let zone = Zone::named(&env::time_zone()).unwrap_or_else(|_| Zone::utc());
    let spawn: Spawn = Rc::new(spawn);
    let later: Later = Rc::new(|ms, f| {
        quench_ui::platform::after(u32::try_from(ms).unwrap_or(u32::MAX), f);
    });
    Rc::new(Web::new(
        Client::new(Fetch, Stored),
        spawn,
        Rc::new(|| Time::from_unix_millis(env::wall_ms())),
        zone,
        later,
    ))
}

/// The app: the screens, started over the controller. The server is the one the page came from.
#[must_use]
pub fn app() -> View {
    let web = platform();
    let zone = daybook_host::Platform::zone(&*web);
    let has_session = web.signed_in();
    let controller: Rc<std::cell::RefCell<Option<Rc<Controller>>>> = Rc::default();
    let requests: Rc<dyn Fn(Request)> = {
        let controller = Rc::clone(&controller);
        Rc::new(move |request| {
            let c = controller.borrow().clone();
            if let Some(c) = c {
                c.request(request);
            }
        })
    };
    let Daybook { view, deliver } = daybook(
        requests,
        Setup {
            // The page's own server: the sign-in form is told so and the address field is left as it is.
            ask_server: false,
            server: String::new(),
            list: ListRef::Inbox,
            has_session,
            zone,
            clock: Rc::new(|| Time::from_unix_millis(env::wall_ms())),
            toast_ms: 4000,
        },
    );
    let c = Controller::new(
        web,
        Store::new("/nonexistent"),
        Rc::new(move |m| deliver(m)),
    );
    *controller.borrow_mut() = Some(Rc::clone(&c));
    if has_session {
        c.start();
    }
    view
}

quench_ui_web::app!(app);
