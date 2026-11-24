//! The drafts (messages started and not sent) and the senders a person muted.

use quench_ui::form::Field;
use quench_ui::prelude::*;
use quench_ui::router::link;
use std::rc::Rc;

use super::{Env, area, labelled, problem_view, waiting};
use crate::live::{Request, State};

/// The drafts, the latest first: open one to go on writing, or throw it away.
#[must_use]
pub(crate) fn drafts_page(env: &Env, state: &State) -> View {
    let (env, state) = (env.clone(), state.clone());
    View::dynamic(move || -> View {
        if let Some(view) = problem_view(&env, &state) {
            return view;
        }
        let Some(list) = state.drafts.get() else {
            return waiting(&env);
        };
        let body: View = if list.is_empty() {
            p().attr("id", "empty")
                .class("fui-empty")
                .text(env.t.t("mr_drafts_empty"))
                .into_view()
        } else {
            div()
                .class("mr-list")
                .attr("id", "drafts")
                .children(list.iter().map(|d| {
                    let (ask, id) = (Rc::clone(&env.ask), d.id.clone());
                    let who = if d.to.trim().is_empty() {
                        "—".to_string()
                    } else {
                        d.to.clone()
                    };
                    let subject = if d.subject.trim().is_empty() {
                        env.t.t("mr_drafts_no_subject")
                    } else {
                        d.subject.clone()
                    };
                    div()
                        .class("mr-row")
                        .attr("data-draft", d.id.clone())
                        .child(
                            link(&env.path(&format!("/compose/draft/{}", d.id)))
                                .class("mr-row-link")
                                .child(span().class("mr-from").text(who))
                                .child(span().class("mr-time").text(env.when(d.updated)))
                                .child(
                                    span()
                                        .class("mr-subject")
                                        .child(span().class("mr-subject-text").text(subject)),
                                )
                                .child(
                                    span()
                                        .class("mr-preview")
                                        .text(d.text.lines().next().unwrap_or("").to_string()),
                                ),
                        )
                        .child(
                            button()
                                .attr("type", "button")
                                .class("mr-btn mr-danger mr-small")
                                .attr("data-delete-draft", d.id.clone())
                                .on_click(move || ask(Request::DeleteDraft(id.clone())))
                                .text(env.t.t("mr_drafts_delete")),
                        )
                        .into_view()
                }))
                .into_view()
        };
        div()
            .class("mr-page")
            .child(
                div().class("mr-head").child(
                    h1().attr("id", "list-title")
                        .text(env.t.t("mr_role_drafts")),
                ),
            )
            .child(body)
            .into_view()
    })
}

/// The signature: what is added to the end of the messages they start.
fn signature_card(env: &Env, state: &State) -> View {
    let (env, state) = (env.clone(), state.clone());
    View::dynamic(move || -> View {
        let Some(me) = state.me.get() else {
            return View::Empty;
        };
        let text = Field::new("signature", &me.signature);
        let (ask, value) = (Rc::clone(&env.ask), text.clone());
        form()
            .attr("id", "signature-form")
            .class("mr-card mr-form")
            .on_submit(move || ask(Request::SaveSignature(value.value_untracked())))
            .child(
                div()
                    .class("mr-subtitle")
                    .text(env.t.t("mr_signature_title")),
            )
            .child(labelled(
                "signature",
                &env.t.t("mr_signature_hint"),
                area(&text),
            ))
            .child(
                div().class("mr-actions").child(
                    button()
                        .attr("type", "submit")
                        .attr("id", "save-signature")
                        .class("mr-btn mr-primary")
                        .text(env.t.t("mr_account_save_signature")),
                ),
            )
            .into_view()
    })
}

/// Whether this browser may tell the person about new mail while the tab is in the background. The choice is the browser's: the
/// page's script keeps `data-fui-notify` on the document up to date, and the style shows the line that fits.
fn notification_card(env: &Env) -> View {
    div()
        .class("mr-card")
        .attr("id", "notify-card")
        .child(div().class("mr-subtitle").text(env.t.t("mr_notify_title")))
        .child(p().class("mr-muted").text(env.t.t("mr_notify_hint")))
        .child(
            button()
                .attr("type", "button")
                .attr("id", "notify-enable")
                .attr("data-fui-notify-enable", "")
                .class("mr-btn mr-notify-ask")
                .text(env.t.t("mr_notify_enable")),
        )
        .child(
            p().class("mr-notify-on mr-ok")
                .text(env.t.t("mr_notify_on")),
        )
        .child(
            p().class("mr-notify-denied mr-muted")
                .text(env.t.t("mr_notify_denied")),
        )
        .child(
            p().class("mr-notify-unsupported mr-muted")
                .text(env.t.t("mr_notify_unsupported")),
        )
        .into_view()
}

/// The settings: the signature, and the senders whose mail goes to the archive, with a way to let each back in.
#[must_use]
pub(crate) fn muted_page(env: &Env, state: &State) -> View {
    let (env, state) = (env.clone(), state.clone());
    View::dynamic(move || -> View {
        if let Some(view) = problem_view(&env, &state) {
            return view;
        }
        let Some(list) = state.mutes.get() else {
            return waiting(&env);
        };
        let body: View = if list.is_empty() {
            p().attr("id", "empty")
                .class("fui-empty")
                .text(env.t.t("mr_muted_empty"))
                .into_view()
        } else {
            div()
                .class("mr-list")
                .attr("id", "muted")
                .children(list.iter().map(|address| {
                    let (ask, to_let_in) = (Rc::clone(&env.ask), address.clone());
                    div()
                        .class("mr-row")
                        .attr("data-muted", address.clone())
                        .child(span().class("mr-from mr-grow").text(address.clone()))
                        .child(
                            button()
                                .attr("type", "button")
                                .class("mr-btn mr-small")
                                .attr("data-unmute", address.clone())
                                .on_click(move || ask(Request::Unmute(to_let_in.clone())))
                                .text(env.t.t("mr_unmute")),
                        )
                        .into_view()
                }))
                .into_view()
        };
        div()
            .class("mr-page")
            .child(
                div().class("mr-head").child(
                    h1().attr("id", "list-title")
                        .text(env.t.t("mr_settings_title")),
                ),
            )
            .child(signature_card(&env, &state))
            .child(notification_card(&env))
            .child(div().class("mr-subtitle").text(env.t.t("mr_muted_title")))
            .child(p().class("mr-muted").text(env.t.t("mr_muted_hint")))
            .child(body)
            .into_view()
    })
}
