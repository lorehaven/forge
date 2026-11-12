//! What a page uses: `fetch` with the session cookie, the page's history and its task queue.

use std::rc::Rc;

use genevia_app::api::{Boxed, Host, HttpRequest, HttpResponse, base_of};
use genevia_app::{Setup, genevia};
use quench_ui::prelude::*;
use quench_ui::router::History;
use quench_ui_web::{BrowserHistory, spawn};

/// `fetch` to the service the page came from, with the cookie the sign-in set.
struct Fetch {
    base: String,
}

impl Host for Fetch {
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

    fn spawn(&self, work: Boxed<'static, ()>) {
        spawn(work);
    }

    fn base(&self) -> String {
        self.base.clone()
    }
}

/// The app: the screens over `fetch`. The service is the one the page came from.
#[must_use]
pub fn app() -> View {
    let history = BrowserHistory::new();
    let base = base_of(&history.location().path);
    genevia(Setup {
        history: Rc::new(history),
        host: Rc::new(Fetch { base }),
    })
}

quench_ui_web::app!(app);
