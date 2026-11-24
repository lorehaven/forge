//! Writing a message: new, a reply, a reply to all, or a forward.

use mailroom_core::draft::ComposeDraft;
use mailroom_core::time::{forward_subject, quote, reply_subject};
use mailroom_core::{Contact, Draft, MessageView, Upload};
use quench_ui::form::Field;
use quench_ui::prelude::*;
use quench_ui::router::link;
use std::cell::{Cell, RefCell};
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
    pub bcc: String,
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
                ..Prefill::default()
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

/// What a saved draft starts from: everything it kept.
#[must_use]
pub fn from_draft(d: &Draft) -> Prefill {
    Prefill {
        to: d.to.clone(),
        cc: d.cc.clone(),
        bcc: d.bcc.clone(),
        subject: d.subject.clone(),
        body: d.text.clone(),
        in_reply_to: d.in_reply_to.clone(),
    }
}

/// The text of a form as one string, to tell whether anything changed since it was last kept.
fn snapshot(fields: [&str; 5]) -> String {
    fields.join("\u{1}")
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

/// How long a message waits before it goes, in milliseconds.
pub const UNDO_MS: u32 = 5000;

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
        let opened = (mode == "draft")
            .then(|| state.draft.get().filter(|d| d.id == id))
            .flatten();
        let original = if mode == "new" || mode == "draft" {
            None
        } else {
            state.message.get().filter(|m| m.summary.id == id)
        };
        if (mode == "draft" && opened.is_none())
            || (!matches!(mode.as_str(), "new" | "draft") && original.is_none())
        {
            return waiting(&env);
        }
        let mut start = opened.as_ref().map_or_else(
            || prefill(&mode, original.as_ref(), &me.address, env.offset),
            from_draft,
        );
        // What the person signs with goes at the end of what they start (a saved draft has its own).
        if opened.is_none() && !me.signature.trim().is_empty() {
            start.body = format!("{}\n\n-- \n{}", start.body, me.signature.trim_end());
        }
        let domain = me
            .address
            .split_once('@')
            .map_or("", |(_, d)| d)
            .to_string();
        let to = Field::new("compose-to", &start.to);
        let cc = Field::new("compose-cc", &start.cc);
        let bcc = Field::new("compose-bcc", &start.bcc);
        let subject = Field::new("compose-subject", &start.subject);
        let body = Field::new("compose-body", &start.body);
        let error: Signal<Option<&'static str>> = create_signal(None);
        let more = create_signal(!start.cc.is_empty() || !start.bcc.is_empty());
        let in_reply_to = start.in_reply_to.clone();

        // Keeping what is typed: a moment after the last key, the text goes to the service as a draft, and the id it answers with
        // is used for the next save. One save at a time; a message being sent or thrown away is not kept.
        let draft_id: Signal<Option<String>> = create_signal(opened.as_ref().map(|d| d.id.clone()));
        let last_kept = Rc::new(RefCell::new(snapshot([
            &start.to,
            &start.cc,
            &start.bcc,
            &start.subject,
            &start.body,
        ])));
        let in_flight = Rc::new(Cell::new(false));
        let finished = Rc::new(Cell::new(false));
        let generation = Rc::new(Cell::new(0_u32));
        let save_now: Rc<dyn Fn()> = {
            let (ask, draft_id, last_kept, in_flight, finished) = (
                Rc::clone(&env.ask),
                draft_id.clone(),
                Rc::clone(&last_kept),
                Rc::clone(&in_flight),
                Rc::clone(&finished),
            );
            let (to, cc, bcc, subject, body, in_reply_to) = (
                to.clone(),
                cc.clone(),
                bcc.clone(),
                subject.clone(),
                body.clone(),
                in_reply_to.clone(),
            );
            Rc::new(move || {
                let fields = [
                    to.value_untracked(),
                    cc.value_untracked(),
                    bcc.value_untracked(),
                    subject.value_untracked(),
                    body.value_untracked(),
                ];
                let now = snapshot([&fields[0], &fields[1], &fields[2], &fields[3], &fields[4]]);
                let blank = fields.iter().all(|f| f.trim().is_empty());
                if finished.get() || in_flight.get() || blank || now == *last_kept.borrow() {
                    return;
                }
                *last_kept.borrow_mut() = now;
                in_flight.set(true);
                let [to, cc, bcc, subject, text] = fields;
                ask(Request::SaveDraft {
                    id: draft_id.get_untracked(),
                    input: mailroom_core::DraftInput {
                        to,
                        cc,
                        bcc,
                        subject,
                        text,
                        in_reply_to: in_reply_to.clone(),
                    },
                });
            })
        };
        // The service kept it: remember its id, and keep what was typed meanwhile.
        create_effect({
            let (saved, draft_id, in_flight, save_now) = (
                state.saved.clone(),
                draft_id.clone(),
                Rc::clone(&in_flight),
                Rc::clone(&save_now),
            );
            move || {
                if let Some(kept) = saved.get() {
                    draft_id.set(Some(kept.id));
                    in_flight.set(false);
                    save_now();
                }
            }
        });
        // A refused change (the save, or a send) ends the wait: the next key tries again.
        create_effect({
            let (notice, in_flight, finished) = (
                state.notice.clone(),
                Rc::clone(&in_flight),
                Rc::clone(&finished),
            );
            move || {
                if notice.get().is_some() {
                    in_flight.set(false);
                    finished.set(false);
                }
            }
        });
        create_effect({
            let (fields, generation, save_now) = (
                [
                    to.clone(),
                    cc.clone(),
                    bcc.clone(),
                    subject.clone(),
                    body.clone(),
                ],
                Rc::clone(&generation),
                Rc::clone(&save_now),
            );
            move || {
                for field in &fields {
                    let _ = field.value();
                }
                generation.set(generation.get() + 1);
                let (mine, generation, save_now) = (
                    generation.get(),
                    Rc::clone(&generation),
                    Rc::clone(&save_now),
                );
                let _ = quench_ui::platform::after(1500, move || {
                    if generation.get() == mine {
                        save_now();
                    }
                });
            }
        });
        // Leaving the page keeps the last words too.
        on_cleanup({
            let save_now = Rc::clone(&save_now);
            move || save_now()
        });

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

        // A message is held for a few seconds, so that it can be taken back; leaving the page sends it at once.
        let held: Rc<RefCell<Option<mailroom_core::Compose>>> = Rc::default();
        let holding = create_signal(false);
        let release: Rc<dyn Fn()> = {
            let (ask, held, holding) = (Rc::clone(&env.ask), Rc::clone(&held), holding.clone());
            Rc::new(move || {
                let taken = held.borrow_mut().take();
                if let Some(compose) = taken {
                    holding.set(false);
                    ask(Request::Send(compose));
                }
            })
        };
        on_cleanup({
            let release = Rc::clone(&release);
            move || release()
        });
        let send = {
            let (hold, holding, release) = (Rc::clone(&held), holding.clone(), Rc::clone(&release));
            let (state, error) = (state.clone(), error.clone());
            let (to, cc, bcc, subject, body) = (
                to.clone(),
                cc.clone(),
                bcc.clone(),
                subject.clone(),
                body.clone(),
            );
            let (draft_id, finished) = (draft_id.clone(), Rc::clone(&finished));
            move || {
                let draft = ComposeDraft {
                    draft: draft_id.get_untracked(),
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
                        finished.set(true);
                        *hold.borrow_mut() = Some(compose);
                        holding.set(true);
                        let release = Rc::clone(&release);
                        let _ = quench_ui::platform::after(UNDO_MS, move || release());
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
                            .child({
                                let (ask, draft_id, finished, held) = (
                                    Rc::clone(&env.ask),
                                    draft_id.clone(),
                                    Rc::clone(&finished),
                                    Rc::clone(&held),
                                );
                                link(&env.path("/home"))
                                    .class("mr-btn")
                                    .attr("id", "discard")
                                    .on_click(move || {
                                        // Thrown away: nothing is kept (nor a message waiting to go), and a draft made already goes.
                                        finished.set(true);
                                        held.borrow_mut().take();
                                        if let Some(id) = draft_id.get_untracked() {
                                            ask(Request::DeleteDraft(id));
                                        }
                                    })
                                    .text(env.t.t("mr_compose_discard"))
                            })
                            .child({
                                let (saved, t) = (state.saved.clone(), Rc::clone(&env.t));
                                View::dynamic(move || {
                                    if saved.get().is_some() {
                                        span()
                                            .attr("id", "draft-status")
                                            .class("mr-muted")
                                            .text(t.t("mr_draft_saved"))
                                            .into_view()
                                    } else {
                                        View::Empty
                                    }
                                })
                            })
                            .child(
                                button()
                                    .attr("type", "submit")
                                    .attr("id", "send")
                                    .class("mr-btn mr-primary")
                                    .attr_bool("disabled", {
                                        let holding = holding.clone();
                                        move || holding.get()
                                    })
                                    .text(env.t.t("mr_compose_send")),
                            )
                            .child({
                                let (t, holding, held, finished) = (
                                    Rc::clone(&env.t),
                                    holding.clone(),
                                    Rc::clone(&held),
                                    Rc::clone(&finished),
                                );
                                View::dynamic(move || {
                                    if !holding.get() {
                                        return View::Empty;
                                    }
                                    let (holding, held, finished) =
                                        (holding.clone(), Rc::clone(&held), Rc::clone(&finished));
                                    span()
                                        .class("mr-actions")
                                        .child(span().class("mr-muted").text(t.t("mr_sending")))
                                        .child(
                                            button()
                                                .attr("type", "button")
                                                .attr("id", "undo-send")
                                                .class("mr-btn")
                                                .on_click(move || {
                                                    held.borrow_mut().take();
                                                    holding.set(false);
                                                    finished.set(false);
                                                })
                                                .text(t.t("mr_send_undo")),
                                        )
                                        .into_view()
                                })
                            }),
                    ),
            )
            .into_view()
    })
}
