//! Writing a message: new, a reply, a reply to all, or a forward.

use mailroom_core::draft::ComposeDraft;
use mailroom_core::time::{forward_subject, quote, reply_subject};
use mailroom_core::{Account, Contact, MessageView, Upload};
use quench_ui::form::Field;
use quench_ui::prelude::*;
use quench_ui::router::link;
use std::rc::Rc;

use super::{Env, area, form_error, labelled, problem_view, text_box, waiting};
use crate::live::{Request, State};

/// The result the page's upload control writes into the hidden input: `{name, status, body}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Uploaded {
    Done(Upload),
    Failed(String),
}

/// Reads what the upload control reports.
#[must_use]
pub fn upload_outcome(raw: &str) -> Uploaded {
    let Ok(value) = quench_serde::json::parse(raw) else {
        return Uploaded::Failed("the upload could not be read".into());
    };
    let status = value.get("status").and_then(|v| v.as_u64()).unwrap_or(0);
    let body = value.get("body").and_then(|v| v.as_str()).unwrap_or("");
    if status == 201 {
        return quench_serde::json::from_str::<Upload>(body).map_or_else(
            |_| Uploaded::Failed("the answer was unreadable".into()),
            Uploaded::Done,
        );
    }
    let why = quench_serde::json::from_str::<mailroom_core::ApiError>(body)
        .map(|e| e.error)
        .unwrap_or_else(|_| format!("the upload failed ({status})"));
    Uploaded::Failed(why)
}

/// What a reply, a reply to all or a forward starts from.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Prefill {
    pub to: String,
    pub cc: String,
    pub subject: String,
    pub body: String,
    pub in_reply_to: Option<String>,
    pub account_id: String,
}

/// The starting text of a draft for `mode` (`new`, `reply`, `replyall`, `forward`) from the message it answers, written by
/// the mailbox `me` (so that a reply to all does not include the writer).
#[must_use]
pub fn prefill(mode: &str, original: Option<&MessageView>, me: &[String], offset: i32) -> Prefill {
    let Some(m) = original else {
        return Prefill::default();
    };
    let who = m.summary.from.label().to_string();
    let quoted = quote(&m.text, &who, m.summary.date, offset);
    let mine = |c: &Contact| me.iter().any(|a| a.eq_ignore_ascii_case(&c.address));
    match mode {
        "forward" => Prefill {
            subject: forward_subject(&m.summary.subject),
            body: format!(
                "\n\n---------- Forwarded message ----------\nFrom: {}\nSubject: {}\n\n{}",
                m.summary.from.full(),
                m.summary.subject,
                m.text
            ),
            account_id: m.summary.account_id.clone(),
            ..Prefill::default()
        },
        "replyall" => {
            let first = if m.reply_to.is_empty() {
                m.summary.from.clone()
            } else {
                m.reply_to[0].clone()
            };
            let others: Vec<String> = m
                .summary
                .to
                .iter()
                .chain(&m.cc)
                .filter(|c| !mine(c) && c.address != first.address)
                .map(Contact::full)
                .collect();
            Prefill {
                to: first.full(),
                cc: others.join(", "),
                subject: reply_subject(&m.summary.subject),
                body: quoted,
                in_reply_to: Some(m.summary.id.clone()),
                account_id: m.summary.account_id.clone(),
            }
        }
        _ => {
            let first = if m.reply_to.is_empty() {
                m.summary.from.clone()
            } else {
                m.reply_to[0].clone()
            };
            Prefill {
                to: first.full(),
                subject: reply_subject(&m.summary.subject),
                body: quoted,
                in_reply_to: Some(m.summary.id.clone()),
                account_id: m.summary.account_id.clone(),
                ..Prefill::default()
            }
        }
    }
}

fn account_options(accounts: &[Account]) -> Vec<(String, String)> {
    accounts
        .iter()
        .map(|a| (a.id.clone(), format!("{} <{}>", a.label, a.address)))
        .collect()
}

/// The compose page.
#[must_use]
pub(crate) fn page(env: &Env, state: &State, mode: &str, id: String) -> View {
    let (env, state, mode) = (env.clone(), state.clone(), mode.to_string());
    View::dynamic(move || -> View {
        if let Some(view) = problem_view(&env, &state) {
            return view;
        }
        let accounts = state.accounts.get().unwrap_or_default();
        let original = if mode == "new" {
            None
        } else {
            state.message.get().filter(|m| m.summary.id == id)
        };
        if mode != "new" && original.is_none() {
            return waiting(&env);
        }
        if accounts.is_empty() {
            return div()
                .class("mr-page")
                .child(
                    p().attr("id", "no-mailbox")
                        .class("fui-empty")
                        .text(env.t.t("mr_compose_no_mailbox")),
                )
                .child(
                    link(&env.path("/accounts"))
                        .class("mr-btn mr-primary")
                        .text(env.t.t("mr_account_add")),
                )
                .into_view();
        }
        let mine: Vec<String> = accounts.iter().map(|a| a.address.clone()).collect();
        let start = prefill(&mode, original.as_ref(), &mine, env.offset);
        let chosen = if accounts.iter().any(|a| a.id == start.account_id) {
            start.account_id.clone()
        } else {
            accounts[0].id.clone()
        };
        let from = Field::new("compose-from", &chosen);
        let to = Field::new("compose-to", &start.to);
        let cc = Field::new("compose-cc", &start.cc);
        let bcc = Field::new("compose-bcc", "");
        let subject = Field::new("compose-subject", &start.subject);
        let body = Field::new("compose-body", &start.body);
        let error: Signal<Option<&'static str>> = create_signal(None);
        let more = create_signal(!start.cc.is_empty());
        let in_reply_to = start.in_reply_to.clone();

        let send = {
            let (env, state, error) = (env.clone(), state.clone(), error.clone());
            let (from, to, cc, bcc, subject, body) = (
                from.clone(),
                to.clone(),
                cc.clone(),
                bcc.clone(),
                subject.clone(),
                body.clone(),
            );
            move || {
                let draft = ComposeDraft {
                    account_id: from.value_untracked(),
                    to: to.value_untracked(),
                    cc: cc.value_untracked(),
                    bcc: bcc.value_untracked(),
                    subject: subject.value_untracked(),
                    text: body.value_untracked(),
                    in_reply_to: in_reply_to.clone(),
                    attachments: state
                        .uploads
                        .get_untracked()
                        .iter()
                        .map(|u| u.id.clone())
                        .collect(),
                };
                match draft.validate() {
                    Ok(compose) => {
                        error.set(None);
                        (env.ask)(Request::Send(compose));
                    }
                    Err(why) => error.set(Some(why.message())),
                }
            }
        };

        let upload_report = {
            let (state, error) = (state.clone(), error.clone());
            input()
                .attr("type", "hidden")
                .attr("id", "upload-result")
                .on_input(move |raw| match upload_outcome(raw) {
                    Uploaded::Done(upload) => {
                        error.set(None);
                        state.notice.set(None);
                        state.uploads.update(|list| list.push(upload));
                    }
                    Uploaded::Failed(why) => state.notice.set(Some(why)),
                })
        };
        let picker = input()
            .class("sr-only")
            .attr("type", "file")
            .attr("id", "attach")
            .attr("data-upload-url", env.api("/uploads"))
            .attr("data-upload-result", "upload-result");
        let attached = {
            let (env, state) = (env.clone(), state.clone());
            View::dynamic(move || {
                let list = state.uploads.get();
                if list.is_empty() {
                    return View::Empty;
                }
                div()
                    .class("mr-attachments")
                    .attr("id", "attached")
                    .children(list.into_iter().map(|u| {
                        let (ask, state, upload) = (Rc::clone(&env.ask), state.clone(), u.clone());
                        span()
                            .class("mr-attachment")
                            .attr("data-upload", u.id.clone())
                            .child(span().text(format!(
                                "{} ({})",
                                u.filename,
                                mailroom_core::time::size(u.size)
                            )))
                            .child(
                                button()
                                    .attr("type", "button")
                                    .class("mr-x")
                                    .attr("aria-label", env.t.t("mr_compose_remove_attachment"))
                                    .on_click(move || {
                                        ask(Request::RemoveUpload(upload.id.clone()));
                                        state.uploads.update(|l| l.retain(|x| x.id != upload.id));
                                    })
                                    .text("×"),
                            )
                            .into_view()
                    }))
                    .into_view()
            })
        };
        let more_fields = {
            let (env, cc, bcc, more) = (env.clone(), cc, bcc, more.clone());
            View::dynamic(move || {
                if more.get() {
                    div()
                        .class("mr-row")
                        .child(labelled(
                            "compose-cc",
                            &env.t.t("mr_compose_cc"),
                            text_box(&cc),
                        ))
                        .child(labelled(
                            "compose-bcc",
                            &env.t.t("mr_compose_bcc"),
                            text_box(&bcc),
                        ))
                        .into_view()
                } else {
                    let more = more.clone();
                    button()
                        .attr("type", "button")
                        .attr("id", "more-fields")
                        .class("mr-link-button")
                        .on_click(move || more.set(true))
                        .text(env.t.t("mr_compose_more"))
                        .into_view()
                }
            })
        };

        div()
            .class("mr-page")
            .child(
                h1().attr("id", "compose-title")
                    .text(env.t.t("mr_compose_title")),
            )
            .child(
                form()
                    .attr("id", "compose")
                    .class("mr-card mr-form")
                    .on_submit(send)
                    .child(labelled(
                        "compose-from",
                        &env.t.t("mr_compose_from"),
                        super::choice(&from, account_options(&accounts)),
                    ))
                    .child(labelled(
                        "compose-to",
                        &env.t.t("mr_compose_to"),
                        text_box(&to),
                    ))
                    .child(more_fields)
                    .child(labelled(
                        "compose-subject",
                        &env.t.t("mr_compose_subject"),
                        text_box(&subject),
                    ))
                    .child(labelled(
                        "compose-body",
                        &env.t.t("mr_compose_body"),
                        area(&body),
                    ))
                    .child(attached)
                    .child(form_error(&error))
                    .child(
                        div()
                            .class("mr-actions")
                            .child(
                                element("label")
                                    .class("mr-btn")
                                    .child(forge_shell::icon("paperclip"))
                                    .child(span().text(env.t.t("mr_compose_attach")))
                                    .child(picker),
                            )
                            .child(upload_report)
                            .child(
                                link(&env.path("/home"))
                                    .class("mr-btn")
                                    .text(env.t.t("mr_compose_discard")),
                            )
                            .child(
                                button()
                                    .attr("type", "submit")
                                    .attr("id", "send")
                                    .class("mr-btn mr-primary")
                                    .text(env.t.t("mr_compose_send")),
                            ),
                    ),
            )
            .into_view()
    })
}
