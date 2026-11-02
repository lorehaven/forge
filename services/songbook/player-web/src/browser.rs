//! What a page uses: `fetch` with the session cookie, and the page's history.

use std::cell::RefCell;
use std::rc::Rc;

use quench_ui::prelude::*;
use quench_ui::router::History;
use quench_ui_web::{BrowserHistory, spawn};
use songbook_player::{Msg, Player, Request, Setup, player};

use crate::protocol::{base_of, call_for, interpret, unreachable};

/// Where the player's `deliver` goes once the player exists.
type Inbox = Rc<RefCell<Option<Rc<dyn Fn(Msg)>>>>;

/// The player over `fetch`. The service is the one the page came from, and the cookie is the one the sign-in set.
#[must_use]
pub fn app() -> View {
    let history = BrowserHistory::new();
    let base = base_of(&history.location().path);
    let slot: Inbox = Rc::default();

    let requests: Rc<dyn Fn(Request)> = {
        let (slot, base) = (Rc::clone(&slot), base.clone());
        Rc::new(move |request| {
            let call = call_for(&request);
            let (slot, url) = (Rc::clone(&slot), format!("{base}{}", call.path));
            spawn(async move {
                let mut http = quench_ui_web::Request::new(call.method, &url)
                    .header("Accept", "application/json");
                if let Some(body) = call.body {
                    http = http.header("Content-Type", "application/json").body(body);
                }
                let msg = match http.send().await {
                    Ok(response) => interpret(&request, response.status, &response.text()),
                    Err(e) => Some(unreachable(&e.to_string())),
                };
                let deliver = slot.borrow().clone();
                if let (Some(deliver), Some(msg)) = (deliver, msg) {
                    deliver(msg);
                }
            });
        })
    };
    let Player { view, deliver, .. } = player(
        requests,
        Setup {
            login_url: format!("{base}/ui/login"),
            base,
            history: Rc::new(history),
            // The admin link is hidden on touch devices by the stylesheet, and the service sends them away from /ui/admin.
            mobile: false,
            engine: None,
            sign_out: None,
        },
    );
    *slot.borrow_mut() = Some(deliver);
    view
}

quench_ui_web::app!(app);
