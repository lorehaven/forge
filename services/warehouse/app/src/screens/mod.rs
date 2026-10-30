//! The screens, as functions from the state to a view.

use std::rc::Rc;

use quench_ui::i18n::I18n;
use quench_ui::prelude::*;
use quench_ui::router::{Router, link};

use crate::live::{Request, State};

mod artifacts;
mod crates;
mod docker;
mod files;
mod rivets;

pub(crate) use artifacts::artifacts;
pub(crate) use crates::crates;
pub(crate) use docker::docker;
pub(crate) use files::{browse, storages};
pub(crate) use rivets::rivets;

/// What every screen needs of the app.
#[derive(Clone)]
pub struct Env {
    pub t: Rc<I18n>,
    pub base: String,
    pub ask: Rc<dyn Fn(Request)>,
    pub router: Router,
}

impl Env {
    /// `{base}/ui{rest}`.
    pub(crate) fn path(&self, rest: &str) -> String {
        format!("{}/ui{rest}", self.base)
    }

    /// The address's query parameter `name` (empty when absent). Reading it in a view follows navigation.
    pub(crate) fn query(&self, name: &str) -> String {
        self.router.location().query_get(name).unwrap_or_default()
    }

    /// Puts `name=value` in the address (in place of the history entry), keeping the other parameters. A screen that opens
    /// "the newest version that is not yanked" pins the version it shows before changing it, or the change would move it.
    pub(crate) fn pin(&self, name: &str, value: &str) {
        let location = self.router.location();
        let mut query: Vec<String> = location
            .query_pairs()
            .into_iter()
            .filter(|(n, _)| n != name)
            .map(|(n, v)| {
                format!(
                    "{}={}",
                    warehouse_core::fmt::encode(&n),
                    warehouse_core::fmt::encode(&v)
                )
            })
            .collect();
        query.push(format!("{name}={}", warehouse_core::fmt::encode(value)));
        self.router
            .replace(&format!("{}?{}", location.path, query.join("&")));
    }

    pub(crate) fn ask(&self, request: Request) {
        (self.ask)(request);
    }
}

/// A link to a screen of the app that opens without a page load. Unlike [`link`] it is not "the current page" for every
/// address of the same path: the tree's links differ only by their query.
pub(crate) fn nav(env: &Env, href: &str) -> Element {
    let (router, to) = (env.router.clone(), href.to_string());
    a().attr("href", href.to_string())
        .intercept_click()
        .on_click(move || router.navigate(&to))
}

pub(crate) fn waiting(env: &Env) -> View {
    p().attr("id", "waiting")
        .class("wh-empty")
        .text(env.t.t("ui_loading"))
        .into_view()
}

/// A line that says there is nothing (here).
pub(crate) fn empty(text: String) -> Element {
    p().class("wh-empty").text(text)
}

#[must_use]
pub fn not_found(env: &Env) -> View {
    div()
        .class("wh-page")
        .child(
            p().attr("id", "not-found")
                .class("wh-empty")
                .text(env.t.t("ui_not_found")),
        )
        .child(link(&env.path("/home")).text(env.t.t("ui_home_button")))
        .into_view()
}

/// The registries under "Home", the open one marked: what goes under the brand in the side panel.
#[must_use]
pub(crate) fn sidebar(env: &Env, state: &State) -> View {
    let (env, overview) = (env.clone(), state.overview.clone());
    View::dynamic(move || {
        let _ = env.t.language();
        let features = overview.get().map(|o| o.features).unwrap_or_default();
        let item = |path: &str, key: &str, id: &str| -> Element {
            let at = env.path(path);
            let prefix = at
                .rsplit_once('/')
                .map_or(at.clone(), |(dir, _)| dir.to_string());
            let router = env.router.clone();
            link(&at)
                .class("fui-nav-item")
                .attr("data-nav", id.to_string())
                .class_if("wh-current", move || {
                    let here = router.location().path;
                    here == at || here.starts_with(&format!("{prefix}/"))
                })
                .child(forge_shell::icon(match id {
                    "docker" => "layers",
                    "crates" => "box",
                    "files" => "folder",
                    "artifacts" => "archive",
                    "rivets" => "database",
                    _ => "house",
                }))
                .child(span().text(env.t.t(key)))
        };
        let mut nav = div().child(item("/home", "ui_home_button", "home"));
        for (on, path, key, id) in [
            (
                features.docker,
                "/docker/catalog",
                "ui_nav_docker",
                "docker",
            ),
            (
                features.crates,
                "/crates/catalog",
                "ui_nav_crates",
                "crates",
            ),
            (features.files, "/files/storages", "ui_nav_files", "files"),
            (
                features.artifacts,
                "/artifacts/catalog",
                "ui_nav_artifacts",
                "artifacts",
            ),
            (
                features.rivets,
                "/rivets/catalog",
                "ui_nav_rivets",
                "rivets",
            ),
        ] {
            if on {
                nav = nav.child(item(path, key, id));
            }
        }
        nav.into_view()
    })
}

// ── Home ─────────────────────────────────────────────────────────────────────

#[must_use]
pub(crate) fn home(env: &Env, state: &State) -> View {
    let (env, overview) = (env.clone(), state.overview.clone());
    let list = {
        let env = env.clone();
        move || -> View {
            let Some(overview) = overview.get() else {
                return waiting(&env);
            };
            let f = overview.features;
            let mut cards = div().class("wh-cards");
            let mut any = false;
            for (on, path, title, desc, id) in [
                (
                    f.docker,
                    "/docker/catalog",
                    "ui_service_docker_title",
                    "ui_service_docker_desc",
                    "docker",
                ),
                (
                    f.crates,
                    "/crates/catalog",
                    "ui_service_crates_title",
                    "ui_service_crates_desc",
                    "crates",
                ),
                (
                    f.files,
                    "/files/storages",
                    "ui_service_files_title",
                    "ui_service_files_desc",
                    "files",
                ),
                (
                    f.artifacts,
                    "/artifacts/catalog",
                    "ui_service_artifacts_title",
                    "ui_service_artifacts_desc",
                    "artifacts",
                ),
                (
                    f.rivets,
                    "/rivets/catalog",
                    "ui_service_rivets_title",
                    "ui_service_rivets_desc",
                    "rivets",
                ),
            ] {
                if !on {
                    continue;
                }
                any = true;
                cards = cards.child(
                    link(&env.path(path))
                        .class("wh-card")
                        .attr("data-service", id)
                        .child(
                            div()
                                .class("wh-card-body")
                                .child(div().class("wh-card-title").text(env.t.t(title)))
                                .child(div().class("wh-card-desc").text(env.t.t(desc))),
                        )
                        .child(div().class("wh-card-arrow").text("→")),
                );
            }
            if !any {
                return p()
                    .attr("id", "no-services")
                    .class("wh-empty")
                    .text(env.t.t("ui_home_no_services"))
                    .into_view();
            }
            div()
                .class("wh-section")
                .child(h3().text(env.t.t("ui_home_group_services")))
                .child(cards)
                .into_view()
        }
    };
    div()
        .class("wh-page")
        .child(h1().text(env.t.t("ui_home_title")))
        .child(View::dynamic(list))
        .into_view()
}

// ── Shared pieces ────────────────────────────────────────────────────────────

/// Two panels side by side: a list on the left, details on the right.
pub(crate) fn split(left: Element, right: Element) -> View {
    div()
        .class("wh-split")
        .child(div().class("wh-panel wh-left").child(left))
        .child(div().class("wh-panel wh-right").child(right))
        .into_view()
}

pub(crate) fn panel_title(text: String) -> Element {
    div().class("wh-panel-title").text(text)
}

/// `Metadata for 1.2.3` (or just `Metadata`).
pub(crate) fn metadata_title(env: &Env, which: Option<&str>) -> Element {
    which.map_or_else(
        || panel_title(env.t.t("ui_metadata")),
        |which| panel_title(format!("{} {which}", env.t.t("ui_metadata_for"))),
    )
}

/// A label and its value.
pub(crate) fn meta(label: String, value: impl IntoView) -> Element {
    div()
        .class("wh-meta-row")
        .child(div().class("wh-meta-label").text(label))
        .child(div().class("wh-meta-value").child(value.into_view()))
}

/// A row of plain text.
pub(crate) fn meta_text(label: String, value: &str) -> Element {
    meta(label, span().text(value.to_string()))
}

pub(crate) fn status(env: &Env, yanked: bool) -> Element {
    if yanked {
        span()
            .class("wh-badge wh-warn")
            .attr("data-status", "yanked")
            .text(env.t.t("ui_status_yanked"))
    } else {
        span()
            .class("wh-badge wh-ok")
            .attr("data-status", "active")
            .text(env.t.t("ui_status_active"))
    }
}

/// A small marker before a version: a dot, warm when yanked.
pub(crate) fn dot(yanked: bool) -> Element {
    span().class(if yanked {
        "wh-dot wh-dot-warn"
    } else {
        "wh-dot"
    })
}

/// A line under a form that says why it was not sent (a translation key).
pub(crate) fn form_error(env: &Env, error: &Signal<Option<&'static str>>) -> View {
    let (env, error) = (env.clone(), error.clone());
    View::dynamic(move || {
        let _ = env.t.language();
        error.get().map_or(View::Empty, |key| {
            p().class("wh-error")
                .attr("role", "alert")
                .text(env.t.t(key))
                .into_view()
        })
    })
}

/// Asks to confirm a destructive step: the dialog calls the host when the person says yes.
pub(crate) fn confirm(
    state: &State,
    title: &'static str,
    text: &'static str,
    target: String,
    action: Request,
) {
    state.confirm.set(Some(crate::live::Confirm {
        text,
        target,
        title,
        action,
    }));
}

/// The dialog for [`confirm`], over everything.
#[must_use]
pub(crate) fn confirm_dialog(env: &Env, state: &State) -> View {
    let (env, current) = (env.clone(), state.confirm.clone());
    View::dynamic(move || {
        let Some(open) = current.get() else {
            return View::Empty;
        };
        let close = {
            let current = current.clone();
            move || current.set(None)
        };
        let go = {
            let (env, current, action) = (env.clone(), current.clone(), open.action.clone());
            move || {
                current.set(None);
                env.ask(action.clone());
            }
        };
        div()
            .attr("id", "confirm")
            .class("wh-modal")
            .child(
                button()
                    .attr("type", "button")
                    .attr("aria-label", env.t.t("ui_close"))
                    .class("wh-modal-backdrop")
                    .on_click(close.clone()),
            )
            .child(
                div()
                    .class("wh-modal-content")
                    .attr("role", "dialog")
                    .child(
                        div()
                            .class("wh-modal-header")
                            .child(div().class("wh-modal-title").text(env.t.t(open.title)))
                            .child(
                                button()
                                    .attr("type", "button")
                                    .attr("aria-label", env.t.t("ui_close"))
                                    .class("wh-modal-close")
                                    .on_click(close.clone())
                                    .text("×"),
                            ),
                    )
                    .child(
                        div()
                            .class("wh-modal-body")
                            .child(p().text(env.t.t(open.text)))
                            .child(div().class("wh-confirm-target").text(open.target.clone()))
                            .child(
                                div()
                                    .class("wh-actions")
                                    .child(
                                        button()
                                            .attr("type", "button")
                                            .attr("id", "confirm-cancel")
                                            .class("wh-btn")
                                            .on_click(close)
                                            .text(env.t.t("ui_common_cancel")),
                                    )
                                    .child(
                                        button()
                                            .attr("type", "button")
                                            .attr("id", "confirm-delete")
                                            .class("wh-btn wh-danger")
                                            .on_click(go)
                                            .text(env.t.t("ui_common_delete")),
                                    ),
                            ),
                    ),
            )
            .into_view()
    })
}

/// A checkbox with a label, on `value`.
pub(crate) fn checkbox_field(id: &str, text: String, value: &Signal<bool>) -> Element {
    let (shown, toggle) = (value.clone(), value.clone());
    label()
        .class("wh-check")
        .attr("for", id.to_string())
        .child(
            input()
                .attr("type", "checkbox")
                .attr("id", id.to_string())
                .attr("name", id.to_string())
                .attr_bool("checked", move || shown.get())
                .on_click(move || toggle.update(|v| *v = !*v)),
        )
        .child(span().text(text))
}

/// A label over a control.
pub(crate) fn labelled(id: &str, text: String, control: Element) -> Element {
    div()
        .class("wh-field")
        .child(label().attr("for", id.to_string()).text(text))
        .child(control)
}
