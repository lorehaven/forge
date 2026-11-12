//! The interface of Genevia: the list of trees and the views of one (list, pedigree, fan), with the side panel and the
//! dialogs for changing a tree. The app does no I/O of its own: every call goes through [`api`], over the [`api::Host`]
//! the page installs, so the same screens run in the browser and in tests.

// A page has one thread: the futures of its calls are not `Send`, and need not be.
#![allow(clippy::future_not_send)]
// The screens are long builders of views, with the short names of their signals; module summaries are long too.
#![allow(
    clippy::too_many_lines,
    clippy::many_single_char_names,
    clippy::type_complexity,
    clippy::option_if_let_else,
    clippy::needless_pass_by_value,
    clippy::too_long_first_doc_paragraph
)]

pub mod api;
pub mod dialogs;
pub mod fan;
pub mod home;
pub mod list;
pub mod pedigree;
pub mod shell;
pub mod state;
pub mod util;
pub mod viewport;

use std::rc::Rc;

use quench_ui::prelude::*;
use quench_ui::router::{History, Router, Routes, layout, link};

use crate::state::Shared;

/// The app's own stylesheet, on the tokens of `forge-shell`.
pub const STYLE: &str = include_str!("style.css");

/// What the page hands the app.
#[allow(missing_debug_implementations)] // holds trait objects
pub struct Setup {
    /// Where the page's address lives.
    pub history: Rc<dyn History>,
    /// What sends the app's requests.
    pub host: Rc<dyn api::Host>,
}

/// Every address of the app, under the service's prefix: `<base>/ui` is the list of trees, `<base>/ui/t/<tree>/...` a
/// tree. Build it inside the reactive root, with the router, after the host is installed.
#[must_use]
pub fn routes_view(router: &Router) -> View {
    let sh = Shared::new();
    let (frame_sh, body_sh) = (sh.clone(), sh);
    let prefix = format!("{}/ui", api::base().trim_start_matches('/'));
    let prefix = prefix.trim_start_matches('/').to_string();
    Routes::new()
        .layout(
            layout(&prefix, shell::page)
                .route("", |_| home::home())
                .layout(
                    layout("t/:tree", move |ctx, inner| {
                        shell::frame(ctx, inner, &frame_sh)
                    })
                    .route("*rest", move |ctx| shell::body(ctx, &body_sh)),
                ),
        )
        .not_found(|_| {
            div()
                .class("home")
                .child(h1().text("Nie ma takiej strony"))
                .child(
                    link(&util::home_url())
                        .class("fui-link")
                        .text("Wszystkie drzewa"),
                )
                .into_view()
        })
        .view(router)
}

/// The app: installs the host and builds the screens over the page's history.
#[must_use]
pub fn genevia(setup: Setup) -> View {
    api::install(setup.host);
    let router = Router::new(setup.history);
    routes_view(&router)
}
