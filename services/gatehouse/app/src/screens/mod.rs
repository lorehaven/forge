//! The screens, as functions from the state to a view.

mod account;
mod admin;
mod home;
mod public;

use std::rc::Rc;

use gatehouse_core::{Notice, Query, Tone, segment};
use quench_ui::i18n::I18n;
use quench_ui::prelude::*;
use quench_ui::router::{RouteContext, Router, link};

use crate::live::{Problem, Request};

pub(crate) use account::{account, mfa_enroll};
pub(crate) use admin::{admin_user, admin_users};
pub(crate) use home::home;
pub(crate) use public::{
    accept_invite, confirm_email, forgot_password, login, login_mfa, register, resend,
    reset_password, unsubscribe,
};

/// What every screen needs of the app.
#[derive(Clone)]
pub struct Env {
    pub t: Rc<I18n>,
    pub base: String,
    pub ask: Rc<dyn Fn(Request)>,
}

impl Env {
    /// A page of the service: `{base}/ui{rest}`.
    pub(crate) fn path(&self, rest: &str) -> String {
        format!("{}/ui{rest}", self.base)
    }

    /// The translation of `key`, or `fallback` when the language has none (the service's own words for a new service).
    pub(crate) fn tr_or(&self, key: &str, fallback: &str) -> String {
        let text = self.t.t(key);
        if text == key && !fallback.is_empty() {
            fallback.to_string()
        } else {
            text
        }
    }
}

/// The query of the page's address.
pub(crate) fn query(ctx: &RouteContext) -> Query {
    Query::new(ctx.location().query_pairs())
}

pub(crate) fn waiting(env: &Env) -> View {
    p().attr("id", "waiting")
        .class("gh-empty")
        .text(env.t.t("ui_loading"))
        .into_view()
}

/// The page for an address that is nothing.
#[must_use]
pub fn not_found(env: &Env) -> View {
    div()
        .class("gh-page")
        .child(
            p().attr("id", "not-found")
                .class("gh-empty")
                .text(env.t.t("ui_not_found")),
        )
        .child(link(&env.path("/home")).text(env.t.t("ui_home_title")))
        .into_view()
}

/// A message from a redirect's query: good news or bad.
pub(crate) fn notice(env: &Env, notice: Option<Notice>) -> Option<Element> {
    notice.map(|n| {
        p().attr("id", "notice")
            .class(match n.tone {
                Tone::Ok => "gh-notice ok",
                Tone::Error => "gh-notice error",
            })
            .attr(
                "role",
                if n.tone == Tone::Error {
                    "alert"
                } else {
                    "status"
                },
            )
            .text(env.t.t(n.key))
    })
}

/// What went wrong with the service, above the screen: it could not be reached, or the session ended.
pub(crate) fn banner(env: &Env, problem: Option<Problem>) -> View {
    match problem {
        Some(Problem::Failed(why)) => p()
            .attr("id", "problem")
            .class("gh-notice error")
            .text(why)
            .into_view(),
        Some(Problem::SignedOut) => {
            let path = quench_ui::router::use_router().location().path;
            p().attr("id", "problem")
                .class("gh-notice error")
                .child(span().text(format!("{} ", env.t.t("ui_session_ended"))))
                .child(
                    a().attr("id", "sign-in")
                        .attr(
                            "href",
                            format!("{}?redirect={}", env.path("/login"), segment(&path)),
                        )
                        .text(env.t.t("ui_login_sign_in")),
                )
                .into_view()
        }
        _ => View::Empty,
    }
}

/// The navigation under the brand in the side panel, the open page marked.
#[must_use]
pub(crate) fn sidebar(env: &Env, router: &Router) -> View {
    let (env, router) = (env.clone(), router.clone());
    View::dynamic(move || {
        let here = router.location().path;
        let item = |rest: &str, key: &str, id: &str| {
            let href = env.path(rest);
            let mut item = link(&href)
                .class("fui-nav-item")
                .attr("id", id.to_string())
                .child(forge_shell::icon(match id {
                    "nav-users" => "users",
                    "nav-account" => "user",
                    _ => "grid",
                }))
                .child(span().text(env.t.t(key)));
            if here == href || (rest != "/home" && here.starts_with(&format!("{href}/"))) {
                item = item.attr("aria-current", "page").class("gh-current");
            }
            item
        };
        div()
            .child(item("/home", "ui_home_title", "nav-services"))
            .child(item("/admin/users", "ui_admin_title", "nav-users"))
            .child(item("/account", "ui_account_button", "nav-account"))
            .into_view()
    })
}

/// A labelled input whose name is its id: the building block of every form here.
pub(crate) fn input_row(
    env: &Env,
    id: &str,
    label_key: &str,
    kind: &str,
    autocomplete: &str,
) -> Element {
    input_named(env, id, id, label_key, kind, autocomplete, true)
}

/// A labelled input posted under `name`.
pub(crate) fn input_named(
    env: &Env,
    id: &str,
    name: &str,
    label_key: &str,
    kind: &str,
    autocomplete: &str,
    required: bool,
) -> Element {
    let mut control = input()
        .class("gh-input")
        .attr("type", kind.to_string())
        .attr("id", id.to_string())
        .attr("name", name.to_string())
        .attr("autocomplete", autocomplete.to_string());
    if required {
        control = control.attr("required", "required");
    }
    div()
        .class("gh-field")
        .child(label().attr("for", id.to_string()).text(env.t.t(label_key)))
        .child(control)
}

pub(crate) fn hidden(name: &str, value: &str) -> Element {
    input()
        .attr("type", "hidden")
        .attr("name", name.to_string())
        .attr("value", value.to_string())
}

/// The form's submit button.
pub(crate) fn submit(env: &Env, key: &str) -> Element {
    button()
        .attr("type", "submit")
        .class("gh-btn gh-primary")
        .text(env.t.t(key))
}

/// A form the browser posts itself: the service answers with a redirect.
pub(crate) fn post_form(action: String) -> Element {
    form()
        .attr("method", "post")
        .attr("action", action)
        .class("gh-form")
}

pub(crate) fn hint(env: &Env, key: &str) -> Element {
    p().class("gh-hint").text(env.t.t(key))
}

pub(crate) fn panel(env: &Env, title_key: &str, body: Element) -> Element {
    div()
        .class("gh-panel")
        .child(div().class("gh-panel-title").text(env.t.t(title_key)))
        .child(div().class("gh-meta").child(body))
}
