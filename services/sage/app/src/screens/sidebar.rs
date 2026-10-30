//! The sidebar: new chat, the projects (the open one with its files and conversations) and the history.

use quench_ui::prelude::*;
use quench_ui::router::link;
use sage_core::{ConversationItem, FileInfo, human_bytes};

use super::Env;
use crate::live::{Dialog, State, conversation_url, new_chat_url, project_url};

fn title(env: &Env, conv: &ConversationItem) -> String {
    if conv.title.trim().is_empty() {
        env.t.t("ui_chat_untitled")
    } else {
        conv.title.clone()
    }
}

/// A conversation's row, with its menu.
fn conversation_row(
    env: &Env,
    state: &State,
    conv: &ConversationItem,
    active: &str,
    nested: bool,
) -> Element {
    let is_active = conv.id == active;
    let name = title(env, conv);
    let open = {
        let (state, conv, name) = (state.clone(), conv.clone(), name.clone());
        move || {
            state.dialog.set(Some(Dialog::DeleteConversation {
                id: conv.id.clone(),
                title: name.clone(),
            }));
        }
    };
    let mut classes = String::from("sg-item");
    if nested {
        classes.push_str(" sg-nested");
    }
    if is_active {
        classes.push_str(" sg-item-active");
    }
    div()
        .class(classes)
        .attr("id", format!("history-item-{}", conv.id))
        .child(
            link(&conversation_url(&env.base, conv))
                .class("sg-item-link")
                .attr("title", name.clone())
                .text(name),
        )
        .child(
            element("details")
                .class("fui-dropdown sg-menu")
                .child(
                    element("summary")
                        .class("sg-menu-trigger")
                        .attr("title", env.t.t("ui_common_delete"))
                        .attr("aria-label", env.t.t("ui_common_delete"))
                        .text("⋮"),
                )
                .child(
                    div().class("fui-dropdown-menu").child(
                        button()
                            .attr("type", "button")
                            .class("fui-dropdown-item sg-delete")
                            .on_click(open)
                            .text(env.t.t("ui_common_delete")),
                    ),
                ),
        )
}

fn file_row(env: &Env, state: &State, file: &FileInfo) -> Element {
    let ask_delete = {
        let (state, file) = (state.clone(), file.clone());
        move || {
            state.dialog.set(Some(Dialog::DeleteFile {
                id: file.id.clone(),
                name: file.file_name.clone(),
            }));
        }
    };
    let mut link_el = a()
        .class("sg-item-link sg-file-link")
        .attr(
            "href",
            format!("{}/api/v1/files/{}/download", env.base, file.id),
        )
        .attr(
            "title",
            format!("{} · {}", file.file_name, human_bytes(file.file_size)),
        )
        .child(
            span()
                .class("sg-file-icon")
                .text(if file.is_image() { "▣" } else { "≡" }),
        )
        .child(span().class("sg-file-name").text(file.file_name.clone()));
    if file.status != "ready" {
        link_el = link_el.child(
            span()
                .class(format!("sg-badge sg-badge-{}", file.status))
                .text(env.t.t(&format!(
                    "ui_file_status_{}",
                    if matches!(file.status.as_str(), "processing" | "failed") {
                        file.status.as_str()
                    } else {
                        "uploaded"
                    }
                ))),
        );
    }
    div()
        .class("sg-item sg-nested sg-file")
        .attr("id", format!("file-item-{}", file.id))
        .child(link_el)
        .child(
            element("details")
                .class("fui-dropdown sg-menu")
                .child(
                    element("summary")
                        .class("sg-menu-trigger")
                        .attr("title", env.t.t("ui_common_delete"))
                        .text("⋮"),
                )
                .child(
                    div().class("fui-dropdown-menu").child(
                        button()
                            .attr("type", "button")
                            .class("fui-dropdown-item sg-delete")
                            .on_click(ask_delete)
                            .text(env.t.t("ui_common_delete")),
                    ),
                ),
        )
}

/// A collapsible section header: a chevron, the label, and whatever sits at the end.
fn header(open: &Signal<bool>, id: &str, label: String, end: Option<Element>) -> Element {
    let (toggle, shown) = (open.clone(), open.clone());
    div()
        .class("sg-section-head")
        .child(
            button()
                .attr("type", "button")
                .attr("id", id.to_string())
                .class("sg-section-toggle")
                .attr_fn("aria-expanded", move || shown.get().to_string())
                .on_click(move || toggle.update(|o| *o = !*o))
                .child(span().class("sg-chevron").text("›"))
                .child(forge_shell::icon(match id {
                    "history-toggle" => "clock",
                    "files-toggle" => "file",
                    _ => "folder",
                }))
                .child(span().text(label)),
        )
        .child(end)
}

pub(crate) fn sidebar(env: &Env, state: &State) -> View {
    let projects_open = create_signal(true);
    let history_open = create_signal(true);
    let files_open = create_signal(false);
    let (env, state) = (env.clone(), state.clone());
    View::dynamic(move || {
        // Reading the dialog here closes the menu that opened it.
        let _ = state.dialog.get();
        let active = state.active.get();
        let projects = state.projects.get();
        let convs = state.convs.get();
        let files = state.project_files.get();

        let new_chat = link(&new_chat_url(&env.base, active.project_id.as_deref()))
            .attr("id", "new-chat")
            .class("sg-new-chat")
            .child(forge_shell::icon("plus"))
            .child(span().text(env.t.t("ui_sidebar_new_chat")));

        let new_project = {
            let state = state.clone();
            button()
                .attr("type", "button")
                .attr("id", "new-project")
                .class("sg-btn sg-small")
                .on_click(move || state.dialog.set(Some(Dialog::NewProject)))
                .text(format!("+ {}", env.t.t("ui_sidebar_new")))
        };

        let mut project_list = div().attr("id", "projects").class("sg-section-body");
        for project in &projects {
            let is_open = active.project_id.as_deref() == Some(project.id.as_str());
            project_list = project_list.child(
                div()
                    .class(if is_open {
                        "sg-item sg-project sg-item-active"
                    } else {
                        "sg-item sg-project"
                    })
                    .attr("id", format!("project-{}", project.id))
                    .child(
                        link(&project_url(&env.base, project))
                            .class("sg-item-link")
                            .child(span().class("sg-glyph").text(if is_open {
                                "▾"
                            } else {
                                "▸"
                            }))
                            .child(span().text(project.name.clone())),
                    ),
            );
            if is_open {
                let mut section = div().attr("id", "project-files").child(header(
                    &files_open,
                    "files-toggle",
                    env.t.t("ui_sidebar_files"),
                    Some(span().class("sg-count").text(files.len().to_string())),
                ));
                if files_open.get() {
                    if files.is_empty() {
                        section = section.child(
                            div()
                                .class("sg-empty-note")
                                .attr("id", "files-empty")
                                .text(env.t.t("ui_files_empty_project")),
                        );
                    }
                    for file in &files {
                        section = section.child(file_row(&env, &state, file));
                    }
                }
                project_list = project_list.child(section);
            }
            for conv in convs
                .iter()
                .filter(|c| c.project_id.as_deref() == Some(project.id.as_str()))
            {
                project_list =
                    project_list.child(conversation_row(&env, &state, conv, &active.id, true));
            }
        }

        let mut history_list = div().attr("id", "history").class("sg-section-body");
        for conv in convs.iter().filter(|c| c.project_id.is_none()) {
            history_list =
                history_list.child(conversation_row(&env, &state, conv, &active.id, false));
        }

        div()
            .class("sg-side")
            .child(new_chat)
            .child(header(
                &projects_open,
                "projects-toggle",
                env.t.t("ui_sidebar_projects"),
                Some(new_project),
            ))
            .child(if projects_open.get() {
                project_list.into_view()
            } else {
                View::Empty
            })
            .child(header(
                &history_open,
                "history-toggle",
                env.t.t("ui_sidebar_history"),
                None,
            ))
            .child(if history_open.get() {
                history_list.into_view()
            } else {
                View::Empty
            })
            .into_view()
    })
}
