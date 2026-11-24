//! The screens, as functions from the state to a view.

pub mod admin;
pub mod compose;
pub mod drafts;
pub mod inbox;
pub mod reader;

use std::rc::Rc;

use mailroom_core::{Contact, FOLDERS};
use quench_ui::form::Field;
use quench_ui::i18n::I18n;
use quench_ui::prelude::*;
use quench_ui::router::{Router, link};

use crate::live::{Problem, Request, State};

/// What every screen needs of the app.
#[derive(Clone)]
pub struct Env {
    pub t: Rc<I18n>,
    pub base: String,
    pub ask: Rc<dyn Fn(Request)>,
    /// The current Unix time, and how far the reader's zone is east of UTC.
    pub now: i64,
    pub offset: i32,
}

impl std::fmt::Debug for Env {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Env")
            .field("base", &self.base)
            .finish_non_exhaustive()
    }
}

impl Env {
    pub(crate) fn path(&self, rest: &str) -> String {
        format!("{}/ui{rest}", self.base)
    }

    pub(crate) fn api(&self, rest: &str) -> String {
        format!("{}/api/v1{rest}", self.base)
    }

    /// The time of a list row.
    pub(crate) fn when(&self, seconds: i64) -> String {
        mailroom_core::time::short(seconds, self.now, self.offset)
    }

    /// The full time of a message.
    pub(crate) fn stamp(&self, seconds: i64) -> String {
        mailroom_core::time::stamp(seconds, self.offset)
    }
}

pub(crate) fn waiting(env: &Env) -> View {
    p().attr("id", "waiting")
        .class("fui-empty")
        .text(env.t.t("ui_loading"))
        .into_view()
}

#[must_use]
pub fn not_found(env: &Env) -> View {
    div()
        .class("mr-page")
        .child(
            p().attr("id", "not-found")
                .class("fui-empty")
                .text(env.t.t("ui_not_found")),
        )
        .child(link(&env.path("/home")).text(env.t.t("mr_nav_inbox")))
        .into_view()
}

/// The screen for a problem the page can say (not found), or `None` to go on.
pub(crate) fn problem_view(env: &Env, state: &State) -> Option<View> {
    (state.problem.get() == Some(Problem::NotFound)).then(|| not_found(env))
}

/// A folder's name for people.
pub(crate) fn folder_name(env: &Env, id: &str) -> String {
    env.t.t(&format!("mr_role_{id}"))
}

/// A person for a list: the name, or the address.
pub(crate) fn person(contact: &Contact) -> String {
    if contact.address.is_empty() && contact.name.is_empty() {
        "—".to_string()
    } else {
        contact.label().to_string()
    }
}

/// The side panel: write, the three folders with what is unread, the panel for administrators.
#[must_use]
pub(crate) fn sidebar(env: &Env, state: &State, router: &Router) -> View {
    let (env, state, router) = (env.clone(), state.clone(), router.clone());
    View::dynamic(move || {
        let here = router.location().path;
        let current = |href: &str| here == href || here == format!("{href}/");
        let mut nav = div().child(
            link(&env.path("/compose"))
                .class("fui-nav-item mr-write")
                .attr("id", "nav-compose")
                .attr("data-hotkey", "c")
                .child(forge_shell::icon("pen"))
                .child(span().text(env.t.t("mr_nav_compose"))),
        );
        let counts = state.folders.get();
        let mut subs = div().class("fui-nav-sub");
        for id in FOLDERS {
            let href = env.path(&format!("/folder/{id}"));
            let mut item = link(&href)
                .class("fui-nav-subitem")
                .attr("data-folder", id)
                .child(span().text(folder_name(&env, id)));
            // Only the inbox says how much is waiting: unread mail in "sent" or "trash" is not news.
            let unread = counts.iter().find(|f| f.id == id).map_or(0, |f| f.unread);
            if id == "inbox" && unread > 0 {
                item = item.child(span().class("mr-count").text(unread.to_string()));
            }
            if current(&href) || (id == "inbox" && current(&env.path("/home"))) {
                item = item.attr("aria-current", "page").class("mr-current");
            }
            subs = subs.child(item);
            // Drafts are not a folder of the mailbox, but they sit among them.
            if id == "inbox" {
                let href = env.path("/drafts");
                let kept = counts
                    .iter()
                    .find(|f| f.id == "drafts")
                    .map_or(0, |f| f.total);
                let mut item = link(&href)
                    .class("fui-nav-subitem")
                    .attr("data-folder", "drafts")
                    .child(span().text(folder_name(&env, "drafts")));
                if kept > 0 {
                    item = item.child(span().class("mr-count").text(kept.to_string()));
                }
                if current(&href) {
                    item = item.attr("aria-current", "page").class("mr-current");
                }
                subs = subs.child(item);
            }
        }
        nav = nav.child(subs);
        nav = nav.child(
            link(&env.path("/muted"))
                .class("fui-nav-item")
                .attr("id", "nav-muted")
                .child(forge_shell::icon("list"))
                .child(span().text(env.t.t("mr_nav_muted"))),
        );
        if state.me.get().is_some_and(|me| me.admin) {
            nav = nav.child(
                link(&env.path("/admin"))
                    .class("fui-nav-item")
                    .attr("id", "nav-admin")
                    .child(forge_shell::icon("shield"))
                    .child(span().text(env.t.t("mr_nav_admin"))),
            );
        }
        nav.into_view()
    })
}

pub(crate) fn labelled(id: &str, text: &str, control: Element) -> Element {
    div()
        .class("mr-field")
        .child(label().attr("for", id.to_string()).text(text.to_string()))
        .child(control)
}

pub(crate) fn text_box(field: &Field) -> Element {
    field.input().class("mr-input")
}

pub(crate) fn area(field: &Field) -> Element {
    field.bind(textarea()).class("mr-input mr-area")
}

/// A line under a form that says why it was not sent.
pub(crate) fn form_error(error: &Signal<Option<&'static str>>) -> View {
    let error = error.clone();
    View::dynamic(move || {
        error.get().map_or(View::Empty, |why| {
            p().class("mr-error")
                .attr("role", "alert")
                .text(why)
                .into_view()
        })
    })
}

pub(crate) fn badge(text: &str, tone: &str) -> Element {
    span()
        .class(format!("mr-badge mr-{tone}"))
        .text(text.to_string())
}
