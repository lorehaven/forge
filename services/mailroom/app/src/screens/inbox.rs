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

fn row(
    env: &Env,
    _state: &State,
    message: &MessageSummary,
    selected: &Signal<Vec<String>>,
) -> View {
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
            .attr("data-fui-select", "")
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
        .attr("data-fui-star", "")
        .on_click(move || {
            ask(Request::Change(Change {
                ids: vec![id.clone()],
                flagged: Some(!flagged),
                ..Change::default()
            }));
        })
        .child(forge_shell::icon(if flagged {
            "star-fill"
        } else {
            "star"
        }));
    // In "sent" the row says who it went to; everywhere else, who wrote it.
    let who = if message.folder == "sent" {
        message.to.first().map_or_else(|| "—".to_string(), person)
    } else {
        person(&message.from)
    };
    let subject = if message.subject.is_empty() {
        "—".to_string()
    } else {
        message.subject.clone()
    };
    // Top line: who, and when. Then the subject with its tags. Then the first words of the text.
    div()
        .class(if message.seen {
            "mr-row"
        } else {
            "mr-row mr-unread"
        })
        .attr("data-message", message.id.clone())
        .attr("data-fui-row", "")
        .child(pick)
        .child(star)
        .child(
            link(&env.path(&format!("/message/{}", message.id)))
                .class("mr-row-link")
                .attr("data-fui-open", "")
                .attr("draggable", "false")
                .child(span().class("mr-from").text(who))
                .child(span().class("mr-time").text(env.when(message.date)))
                .child(
                    span()
                        .class("mr-subject")
                        .child(span().class("mr-subject-text").text(subject))
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
                        }),
                )
                .child(span().class("mr-preview").text(message.preview.clone())),
        )
        .into_view()
}

/// What can be done with the selected messages.
fn bulk(
    env: &Env,
    state: &State,
    folder: &str,
    selected: &Signal<Vec<String>>,
    every: &Signal<bool>,
) -> View {
    let (env, state, folder, selected, every) = (
        env.clone(),
        state.clone(),
        folder.to_string(),
        selected.clone(),
        every.clone(),
    );
    View::dynamic(move || {
        let chosen = selected.get();
        // The bar is always there, so the buttons are where the eye expects them; they wait for something to be chosen.
        let none = chosen.is_empty();
        // Every message of the folder, not just the ones on the page: the change names the folder instead of the messages.
        let whole = every.get();
        let count = if whole {
            state.list.get().map_or(chosen.len(), |l| l.total as usize)
        } else {
            chosen.len()
        };
        let act = |id: &'static str, label: String, change: Change, danger: bool| {
            let (ask, selected, every, folder) = (
                Rc::clone(&env.ask),
                selected.clone(),
                every.clone(),
                folder.clone(),
            );
            button()
                .attr("type", "button")
                .attr("id", id)
                .class(if danger { "mr-btn mr-danger" } else { "mr-btn" })
                .attr_bool("disabled", move || none)
                .on_click({
                    let ids = chosen.clone();
                    move || {
                        let target = if every.get_untracked() {
                            Change {
                                all_in: Some(folder.clone()),
                                ..change.clone()
                            }
                        } else {
                            Change {
                                ids: ids.clone(),
                                ..change.clone()
                            }
                        };
                        ask(Request::Change(target));
                        selected.set(Vec::new());
                        every.set(false);
                    }
                })
                .text(label)
        };
        // The box on the left chooses the whole page, or lets go of what was chosen.
        let on_page: Vec<String> = state
            .list
            .get()
            .map(|l| l.items.iter().map(|m| m.id.clone()).collect())
            .unwrap_or_default();
        let whole_page = !on_page.is_empty() && on_page.iter().all(|id| chosen.contains(id));
        let toggle = {
            let (selected, every) = (selected.clone(), every.clone());
            input()
                .attr("type", "checkbox")
                .attr("id", "select-page")
                .class("mr-pick")
                .attr("aria-label", env.t.t("mr_select_page"))
                .attr_bool("checked", move || whole_page)
                .attr_bool("disabled", {
                    let empty = on_page.is_empty();
                    move || empty
                })
                .on_click(move || {
                    every.set(false);
                    if whole_page {
                        selected.set(Vec::new());
                    } else {
                        selected.set(on_page.clone());
                    }
                })
        };
        let mut bar = div()
            .class("mr-actions mr-bulk")
            .attr("id", "bulk")
            .child(toggle)
            .child(
                span()
                    .attr("id", "selected-count")
                    .class("mr-muted")
                    .text(if none {
                        String::new()
                    } else {
                        format!("{count} {}", env.t.t("mr_list_selected"))
                    }),
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
            if folder == "archive" {
                bar = bar.child(act(
                    "bulk-inbox",
                    env.t.t("mr_to_inbox"),
                    Change {
                        move_to: Some("inbox".into()),
                        ..Change::default()
                    },
                    false,
                ));
            } else if folder != "sent" {
                bar = bar.child(act(
                    "bulk-archive",
                    env.t.t("mr_to_archive"),
                    Change {
                        move_to: Some("archive".into()),
                        ..Change::default()
                    },
                    false,
                ));
            }
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

/// The most one "everything in the folder" change reaches (the service's own limit).
const MOST_AT_ONCE: u32 = 5000;

/// A checkbox that chooses the page, and, once the page is chosen and there is more behind it, an offer to choose it all. The
/// offer is made only for a folder as it is, not for a search or a filter: the change would reach more than was listed.
fn select_bar(
    env: &Env,
    state: &State,
    selected: &Signal<Vec<String>>,
    every: &Signal<bool>,
    on_page: Vec<String>,
    total: u32,
) -> View {
    let (env, state, selected, every) =
        (env.clone(), state.clone(), selected.clone(), every.clone());
    View::dynamic(move || {
        let chosen = selected.get();
        let whole_page = !on_page.is_empty() && on_page.iter().all(|id| chosen.contains(id));
        let plain = state
            .asked
            .get()
            .is_some_and(|q| q.text.trim().is_empty() && !q.unread && !q.flagged);
        let more = whole_page && plain && total as usize > on_page.len() && total <= MOST_AT_ONCE;
        let note = if every.get() {
            Some((
                env.t
                    .t("mr_select_all_done")
                    .replace("NUMBER", &total.to_string()),
                env.t.t("mr_select_clear"),
                false,
            ))
        } else if more {
            Some((
                env.t
                    .t("mr_select_page_done")
                    .replace("NUMBER", &on_page.len().to_string()),
                env.t
                    .t("mr_select_everything")
                    .replace("NUMBER", &total.to_string()),
                true,
            ))
        } else {
            None
        };
        div()
            .class("mr-selectbar")
            .child(note.map(|(text, label, take_all)| {
                let (selected, every) = (selected.clone(), every.clone());
                span()
                    .attr("id", "select-note")
                    .class("mr-muted")
                    .child(span().text(format!("{text} ")))
                    .child(
                        button()
                            .attr("type", "button")
                            .attr(
                                "id",
                                if take_all {
                                    "select-everything"
                                } else {
                                    "select-clear"
                                },
                            )
                            .class("mr-link-button")
                            .on_click(move || {
                                if take_all {
                                    every.set(true);
                                } else {
                                    every.set(false);
                                    selected.set(Vec::new());
                                }
                            })
                            .text(label),
                    )
            }))
            .into_view()
    })
}

/// What can be done with the whole folder: read it all, or empty the trash (asked twice, it cannot be undone).
fn folder_actions(env: &Env, folder: &str) -> View {
    let ask = Rc::clone(&env.ask);
    let all = Some(folder.to_string());
    match folder {
        "inbox" | "archive" => button()
            .attr("type", "button")
            .attr("id", "mark-all-read")
            .class("mr-btn")
            .on_click(move || {
                ask(Request::Change(Change {
                    all_in: all.clone(),
                    seen: Some(true),
                    ..Change::default()
                }));
            })
            .text(env.t.t("mr_mark_all_read"))
            .into_view(),
        "trash" => {
            let armed = create_signal(false);
            let (shown, label_env) = (armed.clone(), env.clone());
            button()
                .attr("type", "button")
                .attr("id", "empty-trash")
                .class("mr-btn mr-danger")
                .on_click(move || {
                    if armed.get_untracked() {
                        armed.set(false);
                        ask(Request::Change(Change {
                            all_in: all.clone(),
                            delete: true,
                            ..Change::default()
                        }));
                    } else {
                        armed.set(true);
                    }
                })
                .text_fn(move || {
                    label_env.t.t(if shown.get() {
                        "mr_empty_trash_confirm"
                    } else {
                        "mr_empty_trash"
                    })
                })
                .into_view()
        }
        _ => View::Empty,
    }
}

/// The list page for the folder `folder` (`inbox`, `archive`, `sent` or `trash`).
#[must_use]
pub(crate) fn page(env: &Env, state: &State, folder: String) -> View {
    let (env, state) = (env.clone(), state.clone());
    let search = Field::new("search", "");
    let unread_only = create_signal(false);
    let starred_only = create_signal(false);
    let selected: Signal<Vec<String>> = create_signal(Vec::new());
    // Whether the whole folder is chosen, beyond the page (offered when the page is chosen and there is more).
    let every = create_signal(false);

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
        let (env, search, unread_only, starred_only, ask_for, selected, every) = (
            env.clone(),
            search.clone(),
            unread_only.clone(),
            starred_only.clone(),
            ask_for.clone(),
            selected.clone(),
            every.clone(),
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
                every.set(false);
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
                    .attr("data-hotkey", "/")
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
        let (env, state, search, unread_only, starred_only, ask_for, selected, every) = (
            env.clone(),
            state.clone(),
            search,
            unread_only,
            starred_only,
            ask_for,
            selected.clone(),
            every.clone(),
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
            let rows = list.items.iter().map(|m| row(&env, &state, m, &selected));
            let (older, newer) = (list.offset + PAGE < list.total, list.offset > 0);
            let pager = {
                let (search, unread_only, starred_only, ask_for, selected, every) = (
                    search.clone(),
                    unread_only.clone(),
                    starred_only.clone(),
                    ask_for.clone(),
                    selected.clone(),
                    every.clone(),
                );
                let (offset, total) = (list.offset, list.total);
                let page_to = move |to: u32| {
                    let (search, unread_only, starred_only, ask_for, selected, every) = (
                        search.clone(),
                        unread_only.clone(),
                        starred_only.clone(),
                        ask_for.clone(),
                        selected.clone(),
                        every.clone(),
                    );
                    move || {
                        selected.set(Vec::new());
                        every.set(false);
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
            let on_page: Vec<String> = list.items.iter().map(|m| m.id.clone()).collect();
            let select_bar = select_bar(&env, &state, &selected, &every, on_page, list.total);
            div()
                .child(select_bar)
                .child(
                    div()
                        .class("mr-list")
                        .attr("id", "messages")
                        .attr("data-fui-cursor", "")
                        .children(rows),
                )
                .child(pager)
                .into_view()
        }
    };

    div()
        .class("mr-page")
        .child(
            div()
                .class("mr-head")
                .child(
                    h1().attr("id", "list-title")
                        .text(folder_name(&env, &folder)),
                )
                .child(folder_actions(&env, &folder)),
        )
        .child(toolbar)
        .child(bulk(&env, &state, &folder, &selected, &every))
        .child(View::dynamic(body))
        .into_view()
}
