//! A folder: its messages, a search, what is selected, and what can be done with the selection.

use mailroom_core::{Change, MessageSummary};
use quench_ui::form::Field;
use quench_ui::prelude::*;
use quench_ui::router::link;
use std::rc::Rc;

use super::{Env, badge, folder_name, person, problem_view, waiting};
use crate::live::{ListQuery, Request, State};

/// Messages per page (the service's own default).
pub const PAGE: u32 = 50;

fn row(env: &Env, message: &MessageSummary, selected: &Signal<Vec<String>>) -> View {
    let env = env.clone();
    let ask = Rc::clone(&env.ask);
    let (id, flagged) = (message.id.clone(), message.flagged);
    let pick = {
        let (on, toggle, id) = (selected.clone(), selected.clone(), id.clone());
        let mine = id.clone();
        input()
            .attr("type", "checkbox")
            .class("mr-pick")
            .attr("data-select", id.clone())
            .attr("aria-label", env.t.t("mr_list_select"))
            .attr_bool("checked", move || on.get().contains(&mine))
            .on_click(move || {
                toggle.update(|list| {
                    if let Some(at) = list.iter().position(|x| *x == id) {
                        list.remove(at);
                    } else {
                        list.push(id.clone());
                    }
                });
            })
    };
    let id = message.id.clone();
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
    // In "sent" the row says who it went to; everywhere else, who wrote it.
    let who = if message.folder == "sent" {
        message.to.first().map_or_else(|| "—".to_string(), person)
    } else {
        person(&message.from)
    };
    div()
        .class(if message.seen {
            "mr-row"
        } else {
            "mr-row mr-unread"
        })
        .attr("data-message", message.id.clone())
        .child(pick)
        .child(star)
        .child(
            link(&env.path(&format!("/message/{}", message.id)))
                .class("mr-row-link")
                .attr("draggable", "false")
                .child(span().class("mr-from").text(who))
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
                .child(if message.automated {
                    badge(&env.t.t("mr_automated"), "blue")
                } else {
                    span()
                })
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

/// What can be done with the selected messages.
fn bulk(env: &Env, folder: &str, selected: &Signal<Vec<String>>) -> View {
    let (env, folder, selected) = (env.clone(), folder.to_string(), selected.clone());
    View::dynamic(move || {
        let chosen = selected.get();
        if chosen.is_empty() {
            return View::Empty;
        }
        let act = |id: &'static str, label: String, change: Change, danger: bool| {
            let (ask, selected) = (Rc::clone(&env.ask), selected.clone());
            button()
                .attr("type", "button")
                .attr("id", id)
                .class(if danger { "mr-btn mr-danger" } else { "mr-btn" })
                .on_click({
                    let ids = chosen.clone();
                    move || {
                        ask(Request::Change(Change {
                            ids: ids.clone(),
                            ..change.clone()
                        }));
                        selected.set(Vec::new());
                    }
                })
                .text(label)
        };
        let mut bar = div()
            .class("mr-actions mr-bulk")
            .attr("id", "bulk")
            .child(
                span()
                    .attr("id", "selected-count")
                    .class("mr-muted")
                    .text(format!("{} {}", chosen.len(), env.t.t("mr_list_selected"))),
            )
            .child(act(
                "bulk-read",
                env.t.t("mr_mark_read"),
                Change {
                    seen: Some(true),
                    ..Change::default()
                },
                false,
            ))
            .child(act(
                "bulk-unread",
                env.t.t("mr_mark_unread"),
                Change {
                    seen: Some(false),
                    ..Change::default()
                },
                false,
            ))
            .child(act(
                "bulk-star",
                env.t.t("mr_star"),
                Change {
                    flagged: Some(true),
                    ..Change::default()
                },
                false,
            ));
        if folder == "trash" {
            bar = bar
                .child(act(
                    "bulk-restore",
                    env.t.t("mr_list_restore"),
                    Change {
                        move_to: Some("inbox".into()),
                        ..Change::default()
                    },
                    false,
                ))
                .child(act(
                    "bulk-delete",
                    env.t.t("mr_delete_forever"),
                    Change {
                        delete: true,
                        ..Change::default()
                    },
                    true,
                ));
        } else {
            bar = bar.child(act(
                "bulk-trash",
                env.t.t("mr_to_trash"),
                Change {
                    move_to: Some("trash".into()),
                    ..Change::default()
                },
                true,
            ));
        }
        bar.into_view()
    })
}

/// The list page for the folder `folder` (`inbox`, `sent` or `trash`).
#[must_use]
pub(crate) fn page(env: &Env, state: &State, folder: String) -> View {
    let (env, state) = (env.clone(), state.clone());
    let search = Field::new("search", "");
    let unread_only = create_signal(false);
    let starred_only = create_signal(false);
    let selected: Signal<Vec<String>> = create_signal(Vec::new());

    let ask_for = {
        let (env, state, folder) = (env.clone(), state.clone(), folder.clone());
        move |text: String, unread: bool, flagged: bool, offset: u32| {
            let query = ListQuery {
                folder: folder.clone(),
                text,
                unread,
                flagged,
                offset,
            };
            state.asked.set(Some(query.clone()));
            (env.ask)(Request::List(query));
        }
    };

    let toolbar = {
        let (env, search, unread_only, starred_only, ask_for, selected) = (
            env.clone(),
            search.clone(),
            unread_only.clone(),
            starred_only.clone(),
            ask_for.clone(),
            selected.clone(),
        );
        let go = {
            let (search, unread_only, starred_only, ask_for) = (
                search.clone(),
                unread_only.clone(),
                starred_only.clone(),
                ask_for.clone(),
            );
            move || {
                selected.set(Vec::new());
                ask_for(
                    search.value_untracked(),
                    unread_only.get_untracked(),
                    starred_only.get_untracked(),
                    0,
                );
            }
        };
        let toggle = |id: &'static str, label: String, flag: Signal<bool>, go: Rc<dyn Fn()>| {
            let shown = flag.clone();
            button()
                .attr("type", "button")
                .attr("id", id)
                .class("mr-btn")
                .class_if("mr-on", move || shown.get())
                .on_click(move || {
                    flag.update(|u| *u = !*u);
                    go();
                })
                .text(label)
        };
        let go_rc: Rc<dyn Fn()> = Rc::new(go.clone());
        form()
            .attr("id", "search-form")
            .class("mr-toolbar")
            .on_submit(go)
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
            .child(toggle(
                "unread-toggle",
                env.t.t("mr_list_unread_only"),
                unread_only,
                Rc::clone(&go_rc),
            ))
            .child(toggle(
                "starred-toggle",
                env.t.t("mr_list_starred"),
                starred_only,
                go_rc,
            ))
    };

    let body = {
        let (env, state, search, unread_only, starred_only, ask_for, selected) = (
            env.clone(),
            state.clone(),
            search,
            unread_only,
            starred_only,
            ask_for,
            selected.clone(),
        );
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
            let rows = list.items.iter().map(|m| row(&env, m, &selected));
            let (older, newer) = (list.offset + PAGE < list.total, list.offset > 0);
            let pager = {
                let (search, unread_only, starred_only, ask_for, selected) = (
                    search.clone(),
                    unread_only.clone(),
                    starred_only.clone(),
                    ask_for.clone(),
                    selected.clone(),
                );
                let (offset, total) = (list.offset, list.total);
                let page_to = move |to: u32| {
                    let (search, unread_only, starred_only, ask_for, selected) = (
                        search.clone(),
                        unread_only.clone(),
                        starred_only.clone(),
                        ask_for.clone(),
                        selected.clone(),
                    );
                    move || {
                        selected.set(Vec::new());
                        ask_for(
                            search.value_untracked(),
                            unread_only.get_untracked(),
                            starred_only.get_untracked(),
                            to,
                        );
                    }
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
            div().class("mr-head").child(
                h1().attr("id", "list-title")
                    .text(folder_name(&env, &folder)),
            ),
        )
        .child(toolbar)
        .child(bulk(&env, &folder, &selected))
        .child(View::dynamic(body))
        .into_view()
}
