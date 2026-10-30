//! The screens, as functions from the state to a view.

use std::rc::Rc;

use quench_ui::i18n::I18n;
use quench_ui::prelude::*;
use quench_ui::router::link;

use crate::live::{Notice, Problem, Request, State};

mod chat;
mod dialogs;
mod init;
mod markdown;
mod sidebar;

pub(crate) use chat::chat;
pub(crate) use dialogs::dialogs;
pub(crate) use init::initializing;
pub(crate) use sidebar::sidebar;

/// What every screen needs of the app.
#[derive(Clone)]
pub struct Env {
    pub t: Rc<I18n>,
    pub base: String,
    pub ask: Rc<dyn Fn(Request)>,
    pub login: String,
}

impl Env {
    pub(crate) fn path(&self, rest: &str) -> String {
        format!("{}/ui{rest}", self.base)
    }

    /// The service's words for a failure: an `api_error_*` code is translated, anything else shown as it is.
    pub(crate) fn words(&self, text: &str) -> String {
        if text.starts_with("api_error_") || text.starts_with("ui_") {
            self.t.t(text)
        } else {
            text.to_string()
        }
    }
}

#[must_use]
pub fn not_found(env: &Env) -> View {
    div()
        .class("sg-page")
        .child(
            p().attr("id", "not-found")
                .class("fui-empty")
                .text(env.t.t("ui_not_found")),
        )
        .child(link(&env.path("/home")).text(env.t.t("ui_home_button")))
        .into_view()
}

/// What went wrong with the last read or change, above the page.
pub(crate) fn banner(env: &Env, state: &State) -> View {
    let (problem, notice, login, t) = (
        state.problem.clone(),
        state.notice.clone(),
        env.login.clone(),
        Rc::clone(&env.t),
    );
    let env = env.clone();
    View::dynamic(move || match (problem.get(), notice.get()) {
        (Some(Problem::Failed(why)), _) => p()
            .attr("id", "problem")
            .class("fui-notice fui-notice-error")
            .text(why)
            .into_view(),
        (Some(Problem::SignedOut), _) => p()
            .attr("id", "problem")
            .class("fui-notice fui-notice-error")
            .child(span().text(format!("{} ", t.t("ui_session_ended"))))
            .child(
                a().attr("id", "sign-in")
                    .attr("href", login.clone())
                    .text(t.t("ui_sign_in")),
            )
            .into_view(),
        (_, Some(Notice::ErrKey(key))) => p()
            .attr("id", "notice")
            .class("fui-notice fui-notice-error")
            .text(t.t(&key))
            .into_view(),
        (_, Some(Notice::Err(why))) => p()
            .attr("id", "notice")
            .class("fui-notice fui-notice-error")
            .text(env.words(&why))
            .into_view(),
        _ => View::Empty,
    })
}
