//! Writing a message: new, a reply, a reply to all, or a forward.

use mailroom_core::draft::ComposeDraft;
use mailroom_core::time::{forward_subject, quote, reply_subject};
use mailroom_core::{Contact, MessageView, Upload};
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
}

/// The starting text of a draft for `mode` (`new`, `reply`, `replyall`, `forward`) from the message it answers, written by
/// the person whose address is `me` (so that a reply to all does not include the writer, and a reply to something they sent
/// goes to whom they sent it).
#[must_use]
pub fn prefill(mode: &str, original: Option<&MessageView>, me: &str, offset: i32) -> Prefill {
    let Some(m) = original else {
        return Prefill::default();
    };
    let who = m.summary.from.label().to_string();
    let quoted = quote(&m.text, &who, m.summary.date, offset);
    let mine = |c: &Contact| c.address.eq_ignore_ascii_case(me);
    // The ones an answer goes to: the writer, or (when the writer is me) the ones I wrote to.
    let answer_to: Vec<Contact> = if mine(&m.summary.from) && !m.summary.to.is_empty() {
        m.summary.to.clone()
    } else {
        vec![m.summary.from.clone()]
    };
    let full = |list: &[Contact]| {
        list.iter()
            .map(Contact::full)
            .collect::<Vec<_>>()
            .join(", ")
    };
    match mode {
        "forward" => Prefill {
            subject: forward_subject(&m.summary.subject),
            body: format!(
                "\n\n---------- Forwarded message ----------\nFrom: {}\nSubject: {}\n\n{}",
                m.summary.from.full(),
                m.summary.subject,
                m.text
            ),
            ..Prefill::default()
        },
        "replyall" => {
            let others: Vec<Contact> = m
                .summary
                .to
                .iter()
                .chain(&m.cc)
                .filter(|c| !mine(c) && !answer_to.iter().any(|a| a.address == c.address))
                .cloned()
                .collect();
            Prefill {
                to: full(&answer_to),
                cc: full(&others),
                subject: reply_subject(&m.summary.subject),
                body: quoted,
                in_reply_to: Some(m.summary.id.clone()),
            }
        }
        _ => Prefill {
            to: full(&answer_to),
            subject: reply_subject(&m.summary.subject),
            body: quoted,
            in_reply_to: Some(m.summary.id.clone()),
            ..Prefill::default()
        },
    }
}

/// `text` with its last recipient (after the last comma) replaced by `address`, ready for the next one.
#[must_use]
pub fn complete(text: &str, address: &str) -> String {
    let head = text.rfind([',', ';']).map_or("", |at| &text[..=at]);
    if head.is_empty() {
        format!("{address}, ")
    } else {
        format!("{} {address}, ", head.trim_end())
    }
}

/// What is being typed as the last recipient, when it could still become a user name (no `@`, no name part).
#[must_use]
pub fn typing(text: &str) -> Option<String> {
    let token = text.rsplit([',', ';']).next().unwrap_or("").trim();
    (!token.is_empty() && !token.contains(['@', '<', '>', ' ', '"'])).then(|| token.to_string())
}

/// The compose page.
#[must_use]
pub(crate) fn page(env: &Env, state: &State, mode: &str, id: String) -> View {
    let (env, state, mode) = (env.clone(), state.clone(), mode.to_string());
    View::dynamic(move || -> View {
        if let Some(view) = problem_view(&env, &state) {
            return view;
        }
        let Some(me) = state.me.get() else {
            return waiting(&env);
        };
        let original = if mode == "new" {
            None
        } else {
            state.message.get().filter(|m| m.summary.id == id)
        };
        if mode != "new" && original.is_none() {
            return waiting(&env);
        }
        let start = prefill(&mode, original.as_ref(), &me.address, env.offset);
        let domain = me
            .address
            .split_once('@')
            .map_or("", |(_, d)| d)
            .to_string();
        let to = Field::new("compose-to", &start.to);
        let cc = Field::new("compose-cc", &start.cc);
        let bcc = Field::new("compose-bcc", "");
        let subject = Field::new("compose-subject", &start.subject);
        let body = Field::new("compose-body", &start.body);
        let error: Signal<Option<&'static str>> = create_signal(None);
        let more = create_signal(!start.cc.is_empty());
        let in_reply_to = start.in_reply_to.clone();

        // As the last recipient is typed, ask who it could be.
        state.people.set(Vec::new());
        create_effect({
            let (to, ask, people) = (to.clone(), Rc::clone(&env.ask), state.people.clone());
            move || match typing(&to.value()) {
                Some(token) => ask(Request::People(token)),
                None => people.set(Vec::new()),
            }
        });
        let suggestions = {
            let (to, people) = (to.clone(), state.people.clone());
            View::dynamic(move || {
                let list = people.get();
                if list.is_empty() {
                    return View::Empty;
                }
                div()
                    .class("mr-actions")
                    .attr("id", "people")
                    .children(list.into_iter().map(|c| {
                        let (to, people, address) = (to.clone(), people.clone(), c.address.clone());
                        button()
                            .attr("type", "button")
                            .class("mr-btn mr-small")
                            .attr("data-person", c.address.clone())
                            .on_click(move || {
                                to.set(&complete(&to.value_untracked(), &address));
                                people.set(Vec::new());
                            })
                            .text(c.address)
                            .into_view()
                    }))
                    .into_view()
            })
        };

        let send = {
            let (env, state, error) = (env.clone(), state.clone(), error.clone());
            let (to, cc, bcc, subject, body) = (
                to.clone(),
                cc.clone(),
                bcc.clone(),
                subject.clone(),
                body.clone(),
            );
            move || {
                let draft = ComposeDraft {
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
                        "compose-to",
                        &env.t.t("mr_compose_to"),
                        text_box(&to),
                    ))
                    .child(
                        p().attr("id", "compose-hint")
                            .class("mr-muted")
                            .text(format!("{} @{domain}", env.t.t("mr_compose_hint"))),
                    )
                    .child(suggestions)
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
