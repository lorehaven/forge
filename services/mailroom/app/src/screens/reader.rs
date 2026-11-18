//! One message: who, when, its text or its HTML (in a sandboxed frame), its attachments, and what can be done with it.

use mailroom_core::{Change, MessageView};
use quench_ui::prelude::*;
use quench_ui::router::link;
use std::rc::Rc;

use super::{Env, badge, person, problem_view, waiting};
use crate::live::{Request, State};

/// The first `https` address in what the sender gave to stop its mail (`<mailto:...>, <https://...>` or a bare link), if there is one. Only `https` is
/// offered: the link is the sender's, and it is opened in a new window with no way back to this page.
#[must_use]
pub fn unsubscribe_link(header: &str) -> Option<String> {
    header
        .split(',')
        .map(|part| part.trim().trim_start_matches('<').trim_end_matches('>'))
        .find(|url| {
            url.starts_with("https://")
                && url.len() < 2000
                && !url.chars().any(|c| {
                    c.is_whitespace() || c.is_control() || c == '"' || c == '<' || c == '>'
                })
        })
        .map(str::to_string)
}

fn contacts(list: &[mailroom_core::Contact]) -> String {
    list.iter()
        .map(mailroom_core::Contact::full)
        .collect::<Vec<_>>()
        .join(", ")
}

fn header_line(env: &Env, key: &str, text: String) -> Element {
    div()
        .class("mr-header-line")
        .child(span().class("mr-header-key").text(env.t.t(key)))
        .child(span().class("mr-header-value").text(text))
}

fn actions(env: &Env, view: &MessageView, remote: &Signal<bool>) -> Element {
    let id = view.summary.id.clone();
    let go = |mode: &'static str| {
        link(&env.path(&format!("/compose/{mode}/{id}")))
            .class("mr-btn")
            .attr("data-action", mode)
    };
    let seen = view.summary.seen;
    let flagged = view.summary.flagged;
    let change = |key: &'static str, label: String, change: Change| {
        let (ask, id) = (Rc::clone(&env.ask), id.clone());
        button()
            .attr("type", "button")
            .class(if key == "delete" {
                "mr-btn mr-danger"
            } else {
                "mr-btn"
            })
            .attr("data-action", key)
            .on_click(move || {
                ask(Request::Change(Change {
                    ids: vec![id.clone()],
                    ..change.clone()
                }))
            })
            .text(label)
    };
    let in_trash = view.summary.folder == "trash";
    let toggle_unread = change(
        "unread",
        env.t.t(if seen {
            "mr_mark_unread"
        } else {
            "mr_mark_read"
        }),
        Change {
            seen: Some(!seen),
            ..Change::default()
        },
    );
    let star = change(
        "star",
        env.t.t(if flagged { "mr_unstar" } else { "mr_star" }),
        Change {
            flagged: Some(!flagged),
            ..Change::default()
        },
    );
    let restore = in_trash.then(|| {
        change(
            "restore",
            env.t.t("mr_list_restore"),
            Change {
                move_to: Some("inbox".into()),
                ..Change::default()
            },
        )
    });
    let remove = if in_trash {
        change(
            "delete",
            env.t.t("mr_delete_forever"),
            Change {
                delete: true,
                ..Change::default()
            },
        )
    } else {
        change(
            "delete",
            env.t.t("mr_to_trash"),
            Change {
                move_to: Some("trash".into()),
                ..Change::default()
            },
        )
    };
    let images = (view.remote_images > 0).then(|| {
        let remote = remote.clone();
        button()
            .attr("type", "button")
            .class("mr-btn")
            .attr("id", "show-images")
            .on_click(move || remote.set(true))
            .text(format!(
                "{} ({})",
                env.t.t("mr_show_images"),
                view.remote_images
            ))
    });
    // A service has no mailbox to answer to: an automated message offers no reply.
    let replies = (!view.summary.automated).then(|| {
        div()
            .class("mr-actions")
            .child(go("reply").text(env.t.t("mr_reply")))
            .child(go("replyall").text(env.t.t("mr_reply_all")))
    });
    div()
        .class("mr-actions")
        .child(replies)
        .child(go("forward").text(env.t.t("mr_forward")))
        .child(toggle_unread)
        .child(star)
        .child(restore)
        .child(remove)
        .child(images)
}

/// The sandboxed frame the sanitised HTML is shown in: no script, no same-origin access, links open elsewhere. The address it
/// loads answers with a policy of its own that says the same, so a bug in one is not a bug in both.
fn html_frame(env: &Env, id: &str, remote: &Signal<bool>) -> Element {
    let (api, id, remote) = (env.api(""), id.to_string(), remote.clone());
    element("iframe")
        .attr("id", "message-html")
        .class("mr-frame")
        .attr("sandbox", "allow-popups allow-popups-to-escape-sandbox")
        .attr("referrerpolicy", "no-referrer")
        .attr("title", "message")
        .attr_fn("src", move || {
            format!(
                "{api}/messages/{id}/html{}",
                if remote.get() { "?remote=1" } else { "" }
            )
        })
}

/// The reader.
#[must_use]
pub(crate) fn page(env: &Env, state: &State, id: String) -> View {
    let (env, state) = (env.clone(), state.clone());
    let remote = create_signal(false);
    let marked = create_signal(false);
    let show_text = create_signal(false);
    View::dynamic(move || -> View {
        if let Some(view) = problem_view(&env, &state) {
            return view;
        }
        let Some(message) = state.message.get().filter(|m| m.summary.id == id) else {
            return waiting(&env);
        };
        // Opening a message reads it.
        if !message.summary.seen && !marked.get_untracked() {
            marked.set(true);
            (env.ask)(Request::Change(Change {
                ids: vec![id.clone()],
                seen: Some(true),
                ..Change::default()
            }));
        }
        let back = env.path(&format!("/folder/{}", message.summary.folder));
        let attachments = message.attachments.iter().map(|a| {
            link(&env.api(&format!("/messages/{id}/attachments/{}", a.index)))
                .class("mr-attachment")
                .attr("download", a.filename.clone())
                .attr("rel", "noopener")
                .child(forge_shell::icon("paperclip"))
                .child(span().text(format!(
                    "{} ({})",
                    a.filename,
                    mailroom_core::time::size(a.size)
                )))
                .into_view()
        });
        let has_attachments = !message.attachments.is_empty();
        let unsubscribe = unsubscribe_link(&message.unsubscribe);
        let body: View = if message.has_html && !show_text.get() {
            html_frame(&env, &id, &remote).into_view()
        } else {
            pre()
                .attr("id", "message-text")
                .class("mr-text")
                .text(message.text.clone())
                .into_view()
        };
        let switch = message.has_html.then(|| {
            let (flip, label) = (
                show_text.clone(),
                env.t.t(if show_text.get() {
                    "mr_view_html"
                } else {
                    "mr_view_text"
                }),
            );
            button()
                .attr("type", "button")
                .class("mr-btn mr-small")
                .attr("id", "switch-view")
                .on_click(move || flip.update(|t| *t = !*t))
                .text(label)
        });
        div()
            .class("mr-page mr-reader")
            .child(
                div()
                    .class("mr-crumbs")
                    .child(link(&back).text(env.t.t("mr_back"))),
            )
            .child(
                h1().attr("id", "subject")
                    .text(if message.summary.subject.is_empty() {
                        "—".to_string()
                    } else {
                        message.summary.subject.clone()
                    }),
            )
            .child(actions(&env, &message, &remote))
            .child(
                div()
                    .class("mr-card mr-headers")
                    .child(header_line(&env, "mr_from", message.summary.from.full()))
                    .child(header_line(&env, "mr_to", contacts(&message.summary.to)))
                    .child(if message.cc.is_empty() {
                        div()
                    } else {
                        header_line(&env, "mr_cc", contacts(&message.cc))
                    })
                    .child(header_line(
                        &env,
                        "mr_date",
                        env.stamp(message.summary.date),
                    ))
                    .child(
                        div()
                            .class("mr-meta")
                            .child(if message.summary.answered {
                                badge(&env.t.t("mr_answered"), "green")
                            } else {
                                badge("", "none")
                            })
                            .child(if message.summary.automated {
                                badge(&env.t.t("mr_automated"), "blue")
                            } else {
                                badge("", "none")
                            })
                            .child(unsubscribe.map(|url| {
                                a().attr("id", "unsubscribe")
                                    .attr("href", url)
                                    .attr("target", "_blank")
                                    .attr("rel", "noopener noreferrer nofollow")
                                    .class("mr-link")
                                    .text(env.t.t("mr_unsubscribe"))
                            })),
                    ),
            )
            .child(switch)
            .child(div().class("mr-card mr-body").child(body))
            .child(if has_attachments {
                div()
                    .class("mr-card")
                    .attr("id", "attachments")
                    .child(div().class("mr-subtitle").text(env.t.t("mr_attachments")))
                    .child(div().class("mr-attachments").children(attachments))
            } else {
                div()
            })
            .into_view()
    })
}

/// A person in a header, for tests and the list.
#[must_use]
pub fn who(contact: &mailroom_core::Contact) -> String {
    person(contact)
}
