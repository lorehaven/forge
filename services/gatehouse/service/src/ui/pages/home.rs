//! The estate's front door: every service this deployment offers, in one place.
//!
//! The client draws the cards from
//! `GET /api/v1/ui/dashboard`; this serves the page to a visitor with a session and sends the others to sign in.

use crate::ui::common::ActorOrRedirect;
use quench_http::prelude::{Response, get};

fn page(who: ActorOrRedirect) -> Response {
    match who.or_redirect() {
        Ok(_) => super::client::signed_in(),
        Err(response) => response,
    }
}

#[get("/ui/home")]
pub async fn home(who: ActorOrRedirect) -> Response {
    page(who)
}

#[get("/ui/home/")]
pub async fn home_slash(who: ActorOrRedirect) -> Response {
    page(who)
}

pub fn register_routes() {
    let _ = home as fn(_) -> _;
    let _ = home_slash as fn(_) -> _;
}
