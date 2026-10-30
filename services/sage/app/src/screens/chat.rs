//! The chat: the conversation, its navigator, the composer and the answer being written.

use std::cell::Cell;
use std::rc::Rc;

use quench_ui::event::prevent_default;
use quench_ui::form::Field;
use quench_ui::prelude::*;
use sage_core::{
    DraftError, FileInfo, MessageDraft, MessageNode, Source, human_bytes, parse_file, preview,
    upload_result,
};

use super::Env;
use super::markdown::render;
use crate::live::{Notice, Request, State, Turn};

/// A message's id in the page, for the navigator to scroll to.
fn anchor(id: &str) -> String {
    format!("msg-{id}")
}

fn glyph(text: &str) -> Element {
    span()
        .class("sg-glyph")
        .attr("aria-hidden", "true")
        .text(text)
}

fn status_label(env: &Env, status: &str) -> String {
    match status {
        "ready" | "processing" | "failed" => env.t.t(&format!("ui_file_status_{status}")),
        _ => env.t.t("ui_file_status_uploaded"),
    }
}

/// A file as a chip: staged ones (in the composer) can be retried and removed, sent ones can be downloaded.
fn chip(env: &Env, file: &FileInfo, staged: bool) -> Element {
    let mut chip = div()
        .class(format!("sg-chip sg-chip-{}", file.status))
        .attr("id", format!("chip-{}", file.id))
        .attr("data-file-id", file.id.clone())
        .attr(
            "title",
            format!("{} · {}", file.file_name, human_bytes(file.file_size)),
        );
    chip = chip.child(if file.is_image() {
        img()
            .class("sg-thumb")
            .attr(
                "src",
                format!("{}/api/v1/files/{}/download", env.base, file.id),
            )
            .attr("alt", file.file_name.clone())
            .attr("loading", "lazy")
            .into_view()
    } else {
        glyph("≡").class("sg-chip-icon").into_view()
    });
    chip = chip
        .child(span().class("sg-chip-name").text(file.file_name.clone()))
        .child(
            span()
                .class("sg-chip-size")
                .text(human_bytes(file.file_size)),
        );
    let mut badge = span()
        .class(format!("sg-badge sg-badge-{}", file.status))
        .text(status_label(env, &file.status));
    if let Some(why) = &file.error_message {
        badge = badge.attr("title", why.clone());
    }
    chip = chip.child(badge);
    if staged {
        if file.status == "failed" && !file.is_image() {
            let (ask, id) = (Rc::clone(&env.ask), file.id.clone());
            chip = chip.child(
                button()
                    .attr("type", "button")
                    .class("sg-chip-btn sg-retry")
                    .attr("title", env.t.t("ui_files_retry_tooltip"))
                    .on_click(move || ask(Request::Reprocess(id.clone())))
                    .child(glyph("↻")),
            );
        }
        let (ask, id) = (Rc::clone(&env.ask), file.id.clone());
        chip = chip.child(
            button()
                .attr("type", "button")
                .class("sg-chip-btn sg-remove")
                .attr("title", env.t.t("ui_files_remove_tooltip"))
                .on_click(move || ask(Request::DeleteFile(id.clone())))
                .child(glyph("✕")),
        );
    } else {
        chip = chip.child(
            a().class("sg-chip-btn")
                .attr("title", env.t.t("ui_files_download_tooltip"))
                .attr(
                    "href",
                    format!("{}/api/v1/files/{}/download", env.base, file.id),
                )
                .child(glyph("↓")),
        );
    }
    chip
}

fn attachments(env: &Env, files: &[FileInfo]) -> Option<Element> {
    if files.is_empty() {
        return None;
    }
    let mut row = div().class("sg-attachments");
    for file in files {
        row = row.child(chip(env, file, false));
    }
    Some(row)
}

fn sources(env: &Env, list: &[Source]) -> Option<Element> {
    if list.is_empty() {
        return None;
    }
    let mut block = div().class("sg-sources").child(
        span()
            .class("sg-sources-label")
            .text(env.t.t("ui_chat_sources")),
    );
    for source in list {
        let mut item = div()
            .class("sg-source")
            .child(span().text(source.file_name.clone()));
        if let Some(detail) = &source.detail {
            item = item.child(span().class("sg-muted").text(format!(" · {detail}")));
        } else if let Some(index) = source.chunk_index {
            item = item.child(span().class("sg-muted").text(format!(
                " · {}",
                env.t.tr("ui_chat_source_chunk", &[("index", index.into())])
            )));
        }
        if let Some(score) = source.similarity {
            item = item.child(
                span()
                    .class("sg-score")
                    .text(format!("{:.0}%", score * 100.0)),
            );
        }
        block = block.child(item);
    }
    Some(block)
}

fn status_line(env: &Env, status: &sage_core::Status) -> Element {
    let mut line = div()
        .class("sg-status")
        .child(span().class("sg-spin"))
        .child(span().text(env.t.t(&format!("ui_chat_status_{}", status.stage))));
    if let Some(detail) = &status.detail {
        line = line.child(span().class("sg-status-detail").text(detail.clone()));
    }
    line
}

/// The controls under a stored message.
fn controls(
    env: &Env,
    state: &State,
    node: &MessageNode,
    last: bool,
    last_user: bool,
    busy: bool,
) -> Option<Element> {
    let mut row = div().class("sg-controls");
    let mut any = false;
    if let Some((prev, next)) = node.neighbours() {
        let (current, total) = node.position();
        let switch =
            |label: &'static str, id: &'static str, target: String, shown: &'static str| {
                let ask = Rc::clone(&env.ask);
                let conversation_id = state.active.with_untracked(|a| a.id.clone());
                button()
                    .attr("type", "button")
                    .attr("id", format!("{id}-{}", node.id))
                    .class("sg-btn sg-icon-btn")
                    .attr("title", label)
                    .on_click(move || {
                        ask(Request::Switch {
                            conversation_id: conversation_id.clone(),
                            target: target.clone(),
                        });
                    })
                    .child(glyph(shown))
            };
        row = row.child(
            div()
                .class("sg-branch")
                .child(switch("‹", "prev", prev.to_string(), "‹"))
                .child(
                    span()
                        .class("sg-branch-info")
                        .attr("id", format!("branch-{}", node.id))
                        .text(format!("{current}/{total}")),
                )
                .child(switch("›", "next", next.to_string(), "›")),
        );
        any = true;
    }
    if node.role == "assistant" && last && !busy {
        let (ask, state, id) = (Rc::clone(&env.ask), state.clone(), node.id.clone());
        row = row.child(
            button()
                .attr("type", "button")
                .attr("id", "regenerate")
                .class("sg-btn")
                .on_click(move || {
                    state.active.update(|a| a.resumed = true);
                    state.turn.set(Some(Turn {
                        replaces: Some(id.clone()),
                        ..Turn::default()
                    }));
                    ask(Request::Regenerate(id.clone()));
                })
                .child(glyph("↻"))
                .child(span().text(env.t.t("ui_chat_regenerate"))),
        );
        any = true;
    }
    if node.is_user() && last_user && !busy {
        let (state, id, text) = (state.clone(), node.id.clone(), node.content.clone());
        row = row.child(
            button()
                .attr("type", "button")
                .attr("id", "edit")
                .class("sg-btn")
                .on_click(move || {
                    state.message_edit.set(&text);
                    state.editing.set(Some(id.clone()));
                })
                .child(glyph("✎"))
                .child(span().text(env.t.t("ui_chat_edit"))),
        );
        any = true;
    }
    any.then_some(row)
}

/// The edit form that replaces a message's text.
fn edit_form(env: &Env, state: &State, node: &MessageNode) -> Element {
    let (ask, st, id) = (Rc::clone(&env.ask), state.clone(), node.id.clone());
    let save = move || {
        let text = st.message_edit.value_untracked();
        if text.trim().is_empty() {
            return;
        }
        batch(|| {
            st.editing.set(None);
            st.active.update(|a| a.resumed = true);
            st.turn.set(Some(Turn::default()));
        });
        ask(Request::Edit {
            conversation_id: st.active.with_untracked(|a| a.id.clone()),
            message_id: id.clone(),
            text,
        });
    };
    let cancel = {
        let st = state.clone();
        move || st.editing.set(None)
    };
    let on_enter = save.clone();
    form()
        .attr("id", "edit-form")
        .class("sg-edit")
        .on_submit(save)
        .child(
            state
                .message_edit
                .bind(textarea())
                .class("sg-edit-text")
                .on_keydown(move |k| {
                    if k.key == "Enter" && !k.shift {
                        prevent_default();
                        on_enter();
                    }
                }),
        )
        .child(
            div()
                .class("sg-edit-actions")
                .child(
                    button()
                        .attr("type", "button")
                        .attr("id", "edit-cancel")
                        .class("sg-btn")
                        .on_click(cancel)
                        .text(env.t.t("ui_common_cancel")),
                )
                .child(
                    button()
                        .attr("type", "submit")
                        .attr("id", "edit-save")
                        .class("sg-btn sg-primary")
                        .text(env.t.t("ui_chat_save_submit")),
                ),
        )
}

fn bubble(role: &str, id: &str) -> Element {
    div()
        .class(format!("sg-message sg-{role}"))
        .attr("id", anchor(id))
}

fn stored_message(
    env: &Env,
    state: &State,
    node: &MessageNode,
    flags: (bool, bool, bool),
    editing: bool,
) -> Element {
    let (last, last_user, busy) = flags;
    let role = if node.is_user() { "user" } else { "ai" };
    let mut inner = div().class("sg-inner");
    if editing {
        inner = inner.child(edit_form(env, state, node));
    } else {
        inner = inner.child(div().class("sg-content").children(render(
            env,
            &node.id,
            node.content.trim(),
        )));
    }
    if node.is_user() {
        inner = inner.child(attachments(env, &node.attachments));
    }
    if !node.is_user() {
        inner = inner.child(sources(env, &node.sources));
    }
    if !editing {
        inner = inner.child(controls(env, state, node, last, last_user, busy));
    }
    bubble(role, &node.id).child(inner)
}

/// The answer being written (or its placeholder), and the person's message that is not stored yet.
fn turn_messages(env: &Env, turn: &Turn) -> Vec<View> {
    let mut out = Vec::new();
    if let Some((text, files)) = &turn.user {
        out.push(
            bubble("user", "turn-user")
                .child(
                    div()
                        .class("sg-inner")
                        .child(
                            div()
                                .class("sg-content")
                                .children(render(env, "turn-user", text)),
                        )
                        .child(attachments(env, files)),
                )
                .into_view(),
        );
    }
    let mut inner = div().class("sg-inner");
    if let Some(why) = &turn.error {
        inner = inner.child(
            div()
                .class("sg-content sg-failure")
                .attr("id", "turn-error")
                .child(
                    span()
                        .text(format!("{}: ", env.t.t("ui_chat_error")))
                        .into_view(),
                )
                .child(span().text(env.words(why))),
        );
    } else if turn.answer.is_empty() {
        match &turn.status {
            Some(status) => inner = inner.child(status_line(env, status)),
            None => {
                let key = if turn.replaces.is_some() {
                    "ui_chat_regenerating"
                } else {
                    "ui_chat_thinking"
                };
                inner = inner.child(
                    div()
                        .class("sg-content sg-thinking")
                        .attr("id", "thinking")
                        .text(env.t.t(key)),
                );
            }
        }
    } else {
        inner = inner.child(
            div()
                .class("sg-content")
                .attr("id", "streaming")
                .children(render(env, "turn-ai", &turn.answer)),
        );
        if let Some(status) = &turn.status {
            inner = inner.child(status_line(env, status));
        }
    }
    out.push(bubble("ai", "turn-ai").child(inner).into_view());
    out
}

fn welcome(env: &Env) -> Element {
    bubble("ai", "0").child(
        div().class("sg-inner").child(
            div()
                .class("sg-content")
                .attr("id", "welcome")
                .text(env.t.t("ui_chat_welcome_message")),
        ),
    )
}

/// The conversation: stored messages, then the turn in progress.
fn thread(env: &Env, state: &State) -> View {
    let (busy, replaced) = {
        let turn = state.turn.clone();
        (
            create_memo({
                let turn = turn.clone();
                move || turn.with(Option::is_some)
            }),
            create_memo(move || turn.with(|t| t.as_ref().and_then(|t| t.replaces.clone()))),
        )
    };
    let stored = {
        let (env, state) = (env.clone(), state.clone());
        View::dynamic(move || {
            let messages = state.active.with(|a| a.messages.clone());
            let busy = busy.get();
            let replaced = replaced.get();
            let editing = state.editing.get();
            if messages.is_empty() {
                return if busy {
                    View::Empty
                } else {
                    welcome(&env).into_view()
                };
            }
            let last = messages.len() - 1;
            let last_user = messages.iter().rposition(MessageNode::is_user);
            let mut out = Vec::new();
            for (index, node) in messages.iter().enumerate() {
                if replaced.as_deref() == Some(node.id.as_str()) {
                    continue;
                }
                let is_editing = editing.as_deref() == Some(node.id.as_str());
                out.push(
                    stored_message(
                        &env,
                        &state,
                        node,
                        (index == last, Some(index) == last_user, busy),
                        is_editing,
                    )
                    .into_view(),
                );
            }
            fragment(out)
        })
    };
    let live = {
        let (env, turn) = (env.clone(), state.turn.clone());
        View::dynamic(move || match turn.get() {
            Some(turn) => fragment(turn_messages(&env, &turn)),
            None => View::Empty,
        })
    };
    div()
        .attr("id", "thread")
        .class("sg-thread")
        .child(stored)
        .child(live)
        .into_view()
}

/// One dot per message; a click scrolls to the message.
fn navigator(env: &Env, state: &State) -> View {
    let _ = env;
    let state = state.clone();
    View::dynamic(move || {
        let mut dots = Vec::new();
        let messages = state.active.with(|a| a.messages.clone());
        let turn = state.turn.get();
        let chosen = state.dot.get();
        let busy_replaces = turn.as_ref().and_then(|t| t.replaces.clone());
        let mut entries: Vec<(String, String)> = messages
            .iter()
            .filter(|m| busy_replaces.as_deref() != Some(m.id.as_str()))
            .map(|m| (anchor(&m.id), preview(&m.content)))
            .collect();
        if let Some(turn) = &turn {
            if let Some((text, _)) = &turn.user {
                entries.push((anchor("turn-user"), preview(text)));
            }
            entries.push((anchor("turn-ai"), preview(&turn.answer)));
        }
        let last = entries.len().saturating_sub(1);
        for (index, (target, tip)) in entries.into_iter().enumerate() {
            let active = chosen.as_deref().map_or(index == last, |c| c == target);
            let st = state.clone();
            let mark = target.clone();
            dots.push(
                button()
                    .attr("type", "button")
                    .class(if active {
                        "sg-dot sg-dot-active"
                    } else {
                        "sg-dot"
                    })
                    .attr("data-scroll-to", target)
                    .attr("title", tip)
                    .on_click(move || st.dot.set(Some(mark.clone())))
                    .into_view(),
            );
        }
        div()
            .attr("id", "chat-nav")
            .class("sg-nav")
            .children(dots)
            .into_view()
    })
}

fn select_field(field: &Field, options: Vec<(String, String)>) -> Element {
    let mut control = select().attr("id", field.name().to_string());
    for (value, label) in options {
        let shown = field.clone();
        let mine = value.clone();
        control = control.child(
            option()
                .attr("value", value)
                .attr_bool("selected", move || shown.value() == mine)
                .text(label),
        );
    }
    let field = field.clone();
    control.on_input(move |v| field.set(v))
}

fn composer(env: &Env, state: &State) -> View {
    let send = {
        let (env, state) = (env.clone(), state.clone());
        move || {
            if state.turn.with_untracked(Option::is_some) {
                return;
            }
            let active = state.active.get_untracked();
            let staged = state.staged.get_untracked();
            let draft = MessageDraft {
                text: state.message.value_untracked(),
                conversation_id: active.id.clone(),
                project_id: active.project_id.clone(),
                instance_id: state.model.value_untracked(),
                search_provider: state.provider.value_untracked(),
                file_ids: staged.iter().map(|f| f.id.clone()).collect(),
            };
            if draft.text.trim().is_empty() || active.id.is_empty() {
                return;
            }
            match draft.body() {
                Ok(body) => {
                    batch(|| {
                        state.notice.set(None);
                        state.turn.set(Some(Turn {
                            user: Some((body.message.clone(), staged)),
                            ..Turn::default()
                        }));
                        state.staged.set(Vec::new());
                        state.message.reset("");
                        state.active.update(|a| a.resumed = true);
                    });
                    (env.ask)(Request::Send(body));
                }
                Err(DraftError(key)) => state.notice.set(Some(Notice::ErrKey(key.to_string()))),
            }
        }
    };

    let no_model = {
        let models = state.models.clone();
        move || models.with(|m| m.as_ref().is_none_or(|m| m.instances.is_empty()))
    };

    // Files being processed are read again until they are ready or failed.
    {
        let (staged, ask) = (state.staged.clone(), Rc::clone(&env.ask));
        let scheduled = Rc::new(Cell::new(false));
        create_effect(move || {
            let waiting = staged.with(|l| l.iter().any(FileInfo::in_progress));
            if !waiting || scheduled.get() {
                return;
            }
            scheduled.set(true);
            let (staged, ask, scheduled) = (staged.clone(), Rc::clone(&ask), Rc::clone(&scheduled));
            let _ = quench_ui::platform::after(2000, move || {
                scheduled.set(false);
                let ids: Vec<String> = staged.with_untracked(|l| {
                    l.iter()
                        .filter(|f| f.in_progress())
                        .map(|f| f.id.clone())
                        .collect()
                });
                for id in ids {
                    ask(Request::FileStatus(id));
                }
            });
        });
    }

    let chips = {
        let (env, staged) = (env.clone(), state.staged.clone());
        View::dynamic(move || {
            let files = staged.get();
            div()
                .attr("id", "pending-attachments")
                .class("sg-pending")
                .children(
                    files
                        .iter()
                        .map(|f| chip(&env, f, true).into_view())
                        .collect::<Vec<_>>(),
                )
                .into_view()
        })
    };

    let warning = {
        let (env, no_model) = (env.clone(), no_model.clone());
        View::show(no_model, move || {
            div()
                .attr("id", "no-model")
                .class("sg-no-model")
                .text(env.t.t("ui_chat_no_model_available"))
                .into_view()
        })
    };

    let keys = send.clone();
    let area = state
        .message
        .bind(textarea())
        .class("sg-input")
        .attr("rows", "1")
        .attr("placeholder", env.t.t("ui_chat_input_placeholder"))
        .attr_bool("disabled", {
            let no_model = no_model.clone();
            move || no_model()
        })
        .on_keydown(move |k| {
            if k.key == "Enter" && !k.shift {
                prevent_default();
                keys();
            }
        });
    let send_button = button()
        .attr("type", "submit")
        .attr("id", "send")
        .class("sg-send")
        .attr("title", env.t.t("ui_chat_send_button"))
        .attr_bool("disabled", {
            let no_model = no_model.clone();
            move || no_model()
        })
        .child(glyph("↑"));

    // The shell's script posts the chosen file and reports back through this input.
    let upload_result_input = {
        let (state, env) = (state.clone(), env.clone());
        input()
            .attr("type", "hidden")
            .attr("id", "upload-result")
            .on_input(move |raw| {
                let Some((name, status, body)) = upload_result(raw) else {
                    return;
                };
                let file = (status == 201).then(|| parse_file(&body)).flatten();
                match file {
                    Some(file) => state.staged.update(|l| l.push(file)),
                    None => {
                        let why = if body.trim().is_empty() {
                            format!("{name}: {status}")
                        } else {
                            body
                        };
                        state.notice.set(Some(Notice::Err(why)));
                    }
                }
                let _ = &env;
            })
    };
    let picker = {
        let (state, env) = (state.clone(), env.clone());
        let (fields_state, accept_state) = (state.clone(), state.clone());
        input()
            .attr("type", "file")
            .attr("id", "composer-file-input")
            .attr(
                "data-upload-url",
                format!("{}/api/v1/files/staged", env.base),
            )
            .attr("data-upload-result", "upload-result")
            .attr_fn("data-upload-fields", move || {
                fields_state.active.with(|a| {
                    let mut fields = format!(r#"{{"conversation_id":"{}""#, a.id);
                    if let Some(p) = &a.project_id {
                        fields.push_str(&format!(r#","project_id":"{p}""#));
                    }
                    fields.push('}');
                    fields
                })
            })
            .attr_fn("accept", move || {
                accept_state
                    .models
                    .with(|m| m.as_ref().map(|m| m.accept.clone()).unwrap_or_default())
            })
    };
    let attach = label()
        .class("sg-attach")
        .attr("id", "attach")
        .attr("for", "composer-file-input")
        .attr("title", env.t.t("ui_chat_attach_tooltip"))
        .child(glyph("+"))
        .child(picker);

    let providers = {
        let (state, models) = (state.clone(), state.models.clone());
        View::dynamic(move || {
            let options = models.with(|m| {
                m.as_ref()
                    .map(|m| {
                        m.search_providers
                            .iter()
                            .map(|p| (p.id.clone(), p.label.clone()))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default()
            });
            select_field(&state.provider, options)
                .class("sg-select")
                .into_view()
        })
    };
    let models = {
        let (env, state, models) = (env.clone(), state.clone(), state.models.clone());
        View::dynamic(move || {
            let view = models.get();
            let options: Vec<(String, String)> = match &view {
                Some(m) if !m.instances.is_empty() => m
                    .instances
                    .iter()
                    .map(|i| (i.id.clone(), i.model.clone()))
                    .collect(),
                Some(m) if !m.switchboard_ok => {
                    vec![(String::new(), env.t.t("ui_chat_switchboard_unavailable"))]
                }
                _ => vec![(String::new(), env.t.t("ui_chat_no_models"))],
            };
            select_field(&state.model, options)
                .class("sg-select")
                .into_view()
        })
    };

    form()
        .attr("id", "chat-form")
        .class("sg-composer")
        .on_submit(send)
        .child(chips)
        .child(warning)
        .child(div().class("sg-input-row").child(area).child(send_button))
        .child(
            div()
                .class("sg-extras")
                .child(attach)
                .child(upload_result_input)
                .child(div().class("sg-grow"))
                .child(providers)
                .child(models),
        )
        .into_view()
}

/// The chat screen.
pub(crate) fn chat(env: &Env, state: &State) -> View {
    div()
        .class("sg-chat")
        .child(
            div()
                .attr("id", "chat-history")
                .class("sg-history")
                .child(thread(env, state)),
        )
        .child(navigator(env, state))
        .child(composer(env, state))
        .into_view()
}
