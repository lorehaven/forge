//! A list of messages: the unified inbox, or one folder.

use mailroom_core::{Change, MessageSummary};
use quench_ui::form::Field;
use quench_ui::prelude::*;
use quench_ui::router::link;
use std::rc::Rc;

use super::{Env, folder_name, person, problem_view, waiting};
use crate::live::{ListQuery, Request, State};

/// Messages per page (the service's own default).
pub const PAGE: u32 = 50;

fn title(env: &Env, state: &State, folder: Option<&String>) -> String {
    let Some(id) = folder else {
        return env.t.t("mr_nav_all_inboxes");
    };
    let folders = state.folders.get();
    let accounts = state.accounts.get().unwrap_or_default();
    for (account, list) in &folders {
        if let Some(found) = list.iter().find(|f| f.id == *id) {
            let label = accounts
                .iter()
                .find(|a| a.id == *account)
                .map(|a| a.label.clone())
                .unwrap_or_default();
            return format!("{label} · {}", folder_name(env, found));
        }
    }
    env.t.t("mr_nav_inbox")
}

fn row(env: &Env, message: &MessageSummary) -> View {
    let env = env.clone();
    let ask = Rc::clone(&env.ask);
    let (id, flagged) = (message.id.clone(), message.flagged);
    let star = button()
        .attr("type", "button")
        .class(if flagged { "mr-star mr-on" } else { "mr-star" })
        .attr(
            "title",
            env.t.t(if flagged { "mr_unstar" } else { "mr_star" }),
        )
        .attr("data-star", message.id.clone())
        .on_click(move || {
            ask(Request::Change(Change {
                ids: vec![id.clone()],
                flagged: Some(!flagged),
                ..Change::default()
            }));
        })
        .child(forge_shell::icon("star"));
    let preview = if message.preview.is_empty() {
        String::new()
    } else {
        format!(" — {}", message.preview)
    };
    div()
        .class(if message.seen {
            "mr-row"
        } else {
            "mr-row mr-unread"
        })
        .attr("data-message", message.id.clone())
        .child(star)
        .child(
            link(&env.path(&format!("/message/{}", message.id)))
                .class("mr-row-link")
                .attr("draggable", "false")
                .child(span().class("mr-from").text(person(&message.from)))
                .child(
                    span()
                        .class("mr-subject")
                        .child(span().class("mr-subject-text").text(
                            if message.subject.is_empty() {
                                "—".to_string()
                            } else {
                                message.subject.clone()
                            },
                        ))
                        .child(span().class("mr-preview").text(preview)),
                )
                .child(if message.attachments {
                    span()
                        .class("mr-clip")
                        .child(forge_shell::icon("paperclip"))
                } else {
                    span()
                })
                .child(span().class("mr-time").text(env.when(message.date))),
        )
        .into_view()
}

/// The list page. `folder` is the folder's id, or `None` for every inbox.
#[must_use]
pub(crate) fn page(env: &Env, state: &State, folder: Option<String>) -> View {
    let (env, state) = (env.clone(), state.clone());
    let search = Field::new("search", "");
    let unread_only = create_signal(false);

    let ask_for = {
        let (env, state, folder) = (env.clone(), state.clone(), folder.clone());
        move |text: String, unread: bool, offset: u32| {
            let query = ListQuery {
                folder: folder.clone(),
                role: if folder.is_none() {
                    Some("inbox".into())
                } else {
                    None
                },
                text,
                unread,
                offset,
            };
            state.asked.set(Some(query.clone()));
            (env.ask)(Request::List(query));
        }
    };

    let toolbar = {
        let (env, state, search, unread_only, ask_for, folder) = (
            env.clone(),
            state.clone(),
            search.clone(),
            unread_only.clone(),
            ask_for.clone(),
            folder.clone(),
        );
        let go = {
            let (search, unread_only, ask_for) =
                (search.clone(), unread_only.clone(), ask_for.clone());
            move || ask_for(search.value_untracked(), unread_only.get_untracked(), 0)
        };
        let refresh = {
            let (env, state, folder) = (env.clone(), state.clone(), folder.clone());
            move || {
                let accounts = state.accounts.get_untracked().unwrap_or_default();
                let wanted: Vec<String> = match &folder {
                    Some(id) => state
                        .folders
                        .get_untracked()
                        .iter()
                        .find(|(_, list)| list.iter().any(|f| f.id == *id))
                        .map(|(account, _)| vec![account.clone()])
                        .unwrap_or_default(),
                    None => accounts.iter().map(|a| a.id.clone()).collect(),
                };
                for account in wanted {
                    (env.ask)(Request::Refresh(account));
                }
            }
        };
        form()
            .attr("id", "search-form")
            .class("mr-toolbar")
            .on_submit(go.clone())
            .child(
                search
                    .input()
                    .class("mr-input mr-search")
                    .attr("type", "search")
                    .attr("placeholder", env.t.t("mr_list_search_placeholder")),
            )
            .child(
                button()
                    .attr("type", "submit")
                    .class("mr-btn")
                    .attr("id", "search-button")
                    .text(env.t.t("mr_list_search")),
            )
            .child(
                button()
                    .attr("type", "button")
                    .attr("id", "unread-toggle")
                    .class_if("mr-on", {
                        let unread_only = unread_only.clone();
                        move || unread_only.get()
                    })
                    .class("mr-btn")
                    .on_click({
                        let unread_only = unread_only.clone();
                        move || {
                            unread_only.update(|u| *u = !*u);
                            go();
                        }
                    })
                    .text(env.t.t("mr_list_unread_only")),
            )
            .child(
                button()
                    .attr("type", "button")
                    .attr("id", "refresh")
                    .class("mr-btn")
                    .on_click(refresh)
                    .child(forge_shell::icon("rotate"))
                    .child(span().text(env.t.t("mr_list_refresh"))),
            )
    };

    let body = {
        let (env, state, search, unread_only, ask_for) =
            (env.clone(), state.clone(), search, unread_only, ask_for);
        move || -> View {
            if let Some(view) = problem_view(&env, &state) {
                return view;
            }
            let Some(list) = state.list.get() else {
                return waiting(&env);
            };
            if list.items.is_empty() {
                return p()
                    .attr("id", "empty")
                    .class("fui-empty")
                    .text(env.t.t("mr_list_empty"))
                    .into_view();
            }
            let rows = list.items.iter().map(|m| row(&env, m));
            let (older, newer) = (list.offset + PAGE < list.total, list.offset > 0);
            let pager = {
                let (search, unread_only, ask_for) =
                    (search.clone(), unread_only.clone(), ask_for.clone());
                let (offset, total) = (list.offset, list.total);
                let page_to = move |to: u32| {
                    let (search, unread_only, ask_for) =
                        (search.clone(), unread_only.clone(), ask_for.clone());
                    move || ask_for(search.value_untracked(), unread_only.get_untracked(), to)
                };
                div()
                    .class("mr-pager")
                    .child(span().attr("id", "range").class("mr-muted").text(format!(
                        "{}–{} / {}",
                        offset + 1,
                        (offset + PAGE).min(total),
                        total
                    )))
                    .child(
                        button()
                            .attr("type", "button")
                            .attr("id", "newer")
                            .class("mr-btn")
                            .attr_bool("disabled", move || !newer)
                            .on_click(page_to(offset.saturating_sub(PAGE)))
                            .text(env.t.t("mr_list_newer")),
                    )
                    .child(
                        button()
                            .attr("type", "button")
                            .attr("id", "older")
                            .class("mr-btn")
                            .attr_bool("disabled", move || !older)
                            .on_click(page_to(offset + PAGE))
                            .text(env.t.t("mr_list_older")),
                    )
            };
            div()
                .child(div().class("mr-list").attr("id", "messages").children(rows))
                .child(pager)
                .into_view()
        }
    };

    div()
        .class("mr-page")
        .child(
            div()
                .class("mr-head")
                .child(h1().attr("id", "list-title").text_fn({
                    let (env, state, folder) = (env.clone(), state.clone(), folder);
                    move || title(&env, &state, folder.as_ref())
                })),
        )
        .child(toolbar)
        .child(View::dynamic(body))
        .into_view()
}
