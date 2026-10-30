//! File storages: the list with a storage's settings and the forms to create, edit and remove one, and the file browser.

use quench_ui::form::Field;
use quench_ui::prelude::*;
use warehouse_core::fmt::{PreviewKind, content_type, encode, format_bytes, percent, preview_kind};
use warehouse_core::tree::{
    FileNode, Selection, file_tree, last_segment, normalize_path, parent_dir,
};
use warehouse_core::{
    Browse, DynamicStorage, EditStorageDraft, NewStorageDraft, StaticStorage, Storages,
};

use super::{
    Env, checkbox_field, confirm, empty, form_error, labelled, meta, meta_text, metadata_title,
    nav, panel_title, split, waiting,
};
use crate::live::{Request, State};

fn can_manage(state: &State) -> bool {
    state
        .overview
        .with(|o| o.as_ref().is_some_and(|o| o.can_manage))
}

// ── Storages ─────────────────────────────────────────────────────────────────

#[must_use]
pub(crate) fn storages(env: &Env, state: &State) -> View {
    env.ask(Request::Storages);

    let left = {
        let (env, state) = (env.clone(), state.clone());
        View::dynamic(move || {
            let Some(storages) = state.storages.get() else {
                return waiting(&env);
            };
            if storages.fixed.is_empty() && storages.dynamic.is_empty() {
                return empty(env.t.t("ui_storages_empty"))
                    .attr("id", "no-storages")
                    .into_view();
            }
            let selected = env.query("storage");
            let at =
                |name: &str| format!("{}?storage={}", env.path("/files/storages"), encode(name));
            let class = |name: &str| {
                if selected == name {
                    "wh-link active"
                } else {
                    "wh-link"
                }
            };
            let mut list = ul().class("wh-tree");
            for storage in &storages.fixed {
                list = list.child(
                    li().child(
                        nav(&env, &at(&storage.name))
                            .class(class(&storage.name))
                            .attr("data-storage", storage.name.clone())
                            .child(span().text(storage.name.clone()))
                            .child(
                                span()
                                    .class("wh-badge")
                                    .text(env.t.t("ui_storage_static_badge")),
                            ),
                    ),
                );
            }
            for storage in &storages.dynamic {
                list = list.child(
                    li().child(
                        nav(&env, &at(&storage.name))
                            .class(class(&storage.name))
                            .attr("data-storage", storage.name.clone())
                            .child(span().text(storage.name.clone()))
                            .child(
                                span()
                                    .class("wh-faint")
                                    .text(format!(" · {}", storage.owner)),
                            ),
                    ),
                );
            }
            div().class("wh-scroll").child(list).into_view()
        })
    };

    let right = {
        let (env, state) = (env.clone(), state.clone());
        View::dynamic(move || {
            let manage = can_manage(&state);
            let selected = env.query("storage");
            let storages = state.storages.get().unwrap_or_default();
            detail(&env, &state, &storages, &selected, manage).into_view()
        })
    };

    split(
        div()
            .class("wh-pane")
            .child(panel_title(env.t.t("ui_storages_title")))
            .child(left),
        div().class("wh-pane").child(right),
    )
}

fn detail(env: &Env, state: &State, storages: &Storages, selected: &str, manage: bool) -> Element {
    let create = manage.then(|| create_form(env));
    if selected.is_empty() {
        return div()
            .class("wh-detail")
            .child(panel_title(env.t.t("ui_storages_detail_title")))
            .child(
                div()
                    .class("wh-scroll wh-stack")
                    .child(empty(env.t.t("ui_storages_select")))
                    .child(create),
            );
    }
    let title = metadata_title(env, Some(selected));
    let browse = nav(
        env,
        &format!("{}?storage={}", env.path("/files/browse"), encode(selected)),
    )
    .attr("id", "browse-files")
    .class("wh-btn")
    .text(env.t.t("ui_browse_open"));

    let mut body = div().class("wh-scroll wh-stack");
    let mut actions = div().class("wh-actions-row").child(browse);
    if let Some(found) = storages.dynamic_named(selected) {
        body = body.child(dynamic_meta(env, found));
        if manage {
            body = body.child(edit_form(env, found));
            let (state, name) = (state.clone(), found.name.clone());
            actions = actions.child(
                button()
                    .attr("type", "button")
                    .attr("id", "delete-storage")
                    .class("wh-btn wh-danger")
                    .on_click(move || {
                        confirm(
                            &state,
                            "ui_storage_delete_title",
                            "ui_storage_delete_confirm_text",
                            name.clone(),
                            Request::DeleteStorage(name.clone()),
                        );
                    })
                    .text(env.t.t("ui_storage_delete")),
            );
        }
        body = body.child(actions);
    } else if let Some(found) = storages.fixed_named(selected) {
        body = body.child(static_meta(env, found)).child(actions);
    } else if state.storages.with(Option::is_some) {
        body = body.child(empty(env.t.t("ui_storage_not_found")).attr("id", "storage-missing"));
    } else {
        body = body.child(waiting(env));
    }
    body = body.child(create);
    div().class("wh-detail").child(title).child(body)
}

fn static_meta(env: &Env, storage: &StaticStorage) -> Element {
    div()
        .class("wh-meta-list")
        .child(meta_text(env.t.t("ui_storage_kind"), "static"))
        .child(meta_text(env.t.t("ui_storage_root"), &storage.root))
}

fn dynamic_meta(env: &Env, storage: &DynamicStorage) -> Element {
    let pct = percent(storage.used_bytes, storage.quota_bytes);
    let max_file = storage
        .max_file_bytes
        .map_or_else(|| "—".to_string(), format_bytes);
    let sync = if storage.sync_enabled {
        "ui_storage_sync_on"
    } else {
        "ui_storage_sync_off"
    };
    div()
        .class("wh-meta-list")
        .child(meta_text(env.t.t("ui_storage_kind"), "dynamic"))
        .child(meta_text(env.t.t("ui_storage_owner"), &storage.owner))
        .child(meta(
            env.t.t("ui_storage_usage"),
            div()
                .class("wh-quota")
                .child(
                    div()
                        .class("wh-quota-fill")
                        .attr("style", format!("width: {pct:.1}%;")),
                )
                .child(span().class("wh-quota-label").text(format!(
                    "{} / {} ({pct:.1}%)",
                    format_bytes(storage.used_bytes),
                    format_bytes(storage.quota_bytes)
                ))),
        ))
        .child(meta_text(env.t.t("ui_storage_max_file"), &max_file))
        .child(meta(env.t.t("ui_storage_sync"), span().text(env.t.t(sync))))
        .child(meta_text(env.t.t("ui_storage_created"), &storage.created))
}

fn number_box(field: &Field) -> Element {
    field
        .input()
        .class("wh-input")
        .attr("type", "number")
        .attr("min", "0")
        .attr("step", "0.01")
}

fn edit_form(env: &Env, storage: &DynamicStorage) -> Element {
    let draft = EditStorageDraft::from_storage(storage);
    let quota = Field::new("edit-quota", &draft.quota_gib);
    let max_file = Field::new("edit-max-file", &draft.max_file_mib);
    let clear: Signal<bool> = create_signal(draft.clear_max_file);
    let sync: Signal<bool> = create_signal(draft.sync);
    let error: Signal<Option<&'static str>> = create_signal(None);

    let send = {
        let (env, name, error) = (env.clone(), storage.name.clone(), error.clone());
        let (quota, max_file, clear, sync) =
            (quota.clone(), max_file.clone(), clear.clone(), sync.clone());
        move || {
            let draft = EditStorageDraft {
                quota_gib: quota.value_untracked(),
                max_file_mib: max_file.value_untracked(),
                clear_max_file: clear.get_untracked(),
                sync: sync.get_untracked(),
            };
            match draft.validate() {
                Ok(changes) => {
                    error.set(None);
                    env.ask(Request::EditStorage {
                        name: name.clone(),
                        changes,
                    });
                }
                Err(why) => error.set(Some(why.key())),
            }
        }
    };
    form()
        .attr("id", "edit-storage")
        .class("wh-form")
        .on_submit(send)
        .child(
            div()
                .class("wh-subtitle")
                .text(env.t.t("ui_storage_edit_title")),
        )
        .child(labelled(
            "edit-quota",
            env.t.t("ui_storage_quota_gib"),
            number_box(&quota),
        ))
        .child(labelled(
            "edit-max-file",
            env.t.t("ui_storage_max_file_mib"),
            number_box(&max_file),
        ))
        .child(checkbox_field(
            "edit-clear",
            env.t.t("ui_storage_clear_max_file"),
            &clear,
        ))
        .child(checkbox_field(
            "edit-sync",
            env.t.t("ui_storage_sync"),
            &sync,
        ))
        .child(form_error(env, &error))
        .child(
            button()
                .attr("type", "submit")
                .attr("id", "save-storage")
                .class("wh-btn wh-primary")
                .text(env.t.t("ui_storage_save")),
        )
}

fn create_form(env: &Env) -> Element {
    let name = Field::new("new-name", "");
    let owner = Field::new("new-owner", "");
    let quota = Field::new("new-quota", "");
    let max_file = Field::new("new-max-file", "");
    let sync: Signal<bool> = create_signal(false);
    let error: Signal<Option<&'static str>> = create_signal(None);

    let send = {
        let (env, error) = (env.clone(), error.clone());
        let (name, owner, quota, max_file, sync) = (
            name.clone(),
            owner.clone(),
            quota.clone(),
            max_file.clone(),
            sync.clone(),
        );
        move || {
            let draft = NewStorageDraft {
                name: name.value_untracked(),
                owner: owner.value_untracked(),
                quota_gib: quota.value_untracked(),
                max_file_mib: max_file.value_untracked(),
                sync: sync.get_untracked(),
            };
            match draft.validate() {
                Ok(body) => {
                    error.set(None);
                    env.ask(Request::CreateStorage(body));
                }
                Err(why) => error.set(Some(why.key())),
            }
        }
    };
    form()
        .attr("id", "new-storage")
        .class("wh-form")
        .on_submit(send)
        .child(
            div()
                .class("wh-subtitle")
                .text(env.t.t("ui_storage_new_title")),
        )
        .child(labelled(
            "new-name",
            env.t.t("ui_storage_name"),
            name.input().class("wh-input"),
        ))
        .child(labelled(
            "new-owner",
            env.t.t("ui_storage_owner"),
            owner.input().class("wh-input"),
        ))
        .child(labelled(
            "new-quota",
            env.t.t("ui_storage_quota_gib"),
            number_box(&quota),
        ))
        .child(labelled(
            "new-max-file",
            env.t.t("ui_storage_max_file_mib"),
            number_box(&max_file),
        ))
        .child(checkbox_field(
            "new-sync",
            env.t.t("ui_storage_sync"),
            &sync,
        ))
        .child(form_error(env, &error))
        .child(
            button()
                .attr("type", "submit")
                .attr("id", "create-storage")
                .class("wh-btn wh-primary")
                .text(env.t.t("ui_storage_create")),
        )
}

// ── The file browser ─────────────────────────────────────────────────────────

/// The tree of the loaded storage, built once per load.
type Tree = Option<(String, String, FileNode, bool)>;

#[must_use]
pub(crate) fn browse(env: &Env, state: &State) -> View {
    // A storage is the way in; without one the storages list is where one is picked.
    if env.query("storage").is_empty() {
        env.router.replace(&env.path("/files/storages"));
        return View::Empty;
    }
    {
        let env = env.clone();
        create_effect(move || {
            let storage = env.query("storage");
            if !storage.is_empty() {
                untrack(|| env.ask(Request::Browse(storage)));
            }
        });
    }
    // (storage, access, tree, truncated)
    let tree = {
        let state = state.clone();
        create_memo(move || -> Tree {
            state.browse.with(|b| {
                b.as_ref().map(|b| {
                    (
                        b.storage.clone(),
                        b.access.clone(),
                        file_tree(&b.files),
                        b.truncated,
                    )
                })
            })
        })
    };

    let left = {
        let (env, tree) = (env.clone(), tree.clone());
        View::dynamic(move || {
            let storage = env.query("storage");
            let path = normalize_path(&env.query("path"));
            tree.with(|tree| match tree.as_ref().filter(|t| t.0 == storage) {
                None => waiting(&env),
                Some((_, access, _, _)) if access != Browse::READABLE => {
                    empty(access_text(&env, access)).into_view()
                }
                Some((_, _, root, truncated)) => {
                    let mut wrapper = div().class("wh-scroll");
                    wrapper = wrapper.child(
                        ul().class("wh-tree").child(
                            li().child(
                                nav(&env, &browse_at(&env, &storage, ""))
                                    .class(if path.is_empty() {
                                        "wh-link active"
                                    } else {
                                        "wh-link"
                                    })
                                    .attr("data-path", "")
                                    .text(env.t.t("ui_storage_files_title")),
                            ),
                        ),
                    );
                    if root.children.is_empty() {
                        wrapper = wrapper.child(empty(env.t.t("ui_files_empty_dir")));
                    } else {
                        let mut list = ul().class("wh-tree");
                        for (name, node) in root.sorted_children() {
                            if !node.is_leaf() {
                                list = list.child(dir_node(&env, &storage, name, node, "", &path));
                            }
                        }
                        wrapper = wrapper.child(list);
                    }
                    if *truncated {
                        wrapper = wrapper.child(
                            div()
                                .class("wh-truncated")
                                .attr("id", "truncated")
                                .text(env.t.t("ui_storage_files_truncated")),
                        );
                    }
                    wrapper.into_view()
                }
            })
        })
    };

    let right =
        {
            let (env, state) = (env.clone(), state.clone());
            View::dynamic(move || {
                let storage = env.query("storage");
                let path = normalize_path(&env.query("path"));
                let manage = can_manage(&state);
                tree.with(|tree| match tree.as_ref().filter(|t| t.0 == storage) {
                    None => waiting(&env),
                    Some((_, access, root, _)) => {
                        let files_title = || panel_title(env.t.t("ui_storage_files_title"));
                        if access != Browse::READABLE {
                            return div()
                                .class("wh-detail")
                                .child(files_title())
                                .child(
                                    div()
                                        .class("wh-scroll")
                                        .child(empty(access_text(&env, access))),
                                )
                                .into_view();
                        }
                        match root.resolve(&path) {
                            Selection::Missing(_) => div()
                                .class("wh-detail")
                                .child(files_title())
                                .child(
                                    div().class("wh-scroll").child(
                                        empty(env.t.t("api_error_file_not_found"))
                                            .attr("id", "file-missing"),
                                    ),
                                )
                                .into_view(),
                            Selection::Dir(dir) => div()
                                .class("wh-detail")
                                .child(files_title())
                                .child(
                                    div()
                                        .class("wh-scroll wh-stack")
                                        .child(breadcrumbs(&env, &storage, &dir))
                                        .child(listing(&env, &storage, root, &dir)),
                                )
                                .into_view(),
                            Selection::File { path, size } => div()
                                .class("wh-detail")
                                .child(metadata_title(&env, Some(last_segment(&path))))
                                .child(div().class("wh-scroll").child(file_detail(
                                    &env, &state, &storage, &path, size, manage,
                                )))
                                .into_view(),
                        }
                    }
                })
            })
        };

    let back = nav(
        env,
        &format!(
            "{}?storage={}",
            env.path("/files/storages"),
            encode(&env.query("storage"))
        ),
    )
    .attr("id", "back")
    .class("wh-back")
    .text(format!("← {}", env.query("storage")));
    split(
        div()
            .class("wh-pane")
            .child(div().class("wh-panel-title").child(back))
            .child(left),
        div().class("wh-pane").child(right),
    )
}

fn access_text(env: &Env, access: &str) -> String {
    if access == Browse::DENIED {
        env.t.t("api_error_forbidden")
    } else {
        env.t.t("ui_storage_not_found")
    }
}

fn browse_at(env: &Env, storage: &str, path: &str) -> String {
    if path.is_empty() {
        format!("{}?storage={}", env.path("/files/browse"), encode(storage))
    } else {
        format!(
            "{}?storage={}&path={}",
            env.path("/files/browse"),
            encode(storage),
            encode(path)
        )
    }
}

/// A directory of the tree: a folder with the directories below it when there are some.
fn dir_node(
    env: &Env,
    storage: &str,
    name: &str,
    node: &FileNode,
    parent: &str,
    selected: &str,
) -> Element {
    let full = if parent.is_empty() {
        name.to_string()
    } else {
        format!("{parent}/{name}")
    };
    let here = nav(env, &browse_at(env, storage, &full))
        .class(if selected == full {
            "wh-link active"
        } else {
            "wh-link"
        })
        .attr("data-path", full.clone())
        .text(name.to_string());
    let subdirs: Vec<_> = node
        .sorted_children()
        .into_iter()
        .filter(|(_, n)| !n.is_leaf())
        .collect();
    if subdirs.is_empty() {
        return li().child(div().class("wh-folder").child(here));
    }
    let mut details = element("details").attr("data-path", full.clone());
    if selected == full || selected.starts_with(&format!("{full}/")) {
        details = details.attr("open", "open");
    }
    let mut list = ul().class("wh-tree");
    for (child, child_node) in subdirs {
        list = list.child(dir_node(env, storage, child, child_node, &full, selected));
    }
    li().child(
        details
            .child(element("summary").class("wh-folder").child(here))
            .child(list),
    )
}

fn breadcrumbs(env: &Env, storage: &str, path: &str) -> Element {
    let mut crumbs = div().class("wh-crumbs").child(
        nav(env, &browse_at(env, storage, ""))
            .attr("data-crumb", "")
            .text(storage.to_string()),
    );
    let mut so_far = String::new();
    for segment in path.split('/').filter(|s| !s.is_empty()) {
        so_far = if so_far.is_empty() {
            segment.to_string()
        } else {
            format!("{so_far}/{segment}")
        };
        crumbs = crumbs.child(span().class("wh-faint").text(" / ")).child(
            nav(env, &browse_at(env, storage, &so_far))
                .attr("data-crumb", so_far.clone())
                .text(segment.to_string()),
        );
    }
    crumbs
}

/// The directory's entries, directories first, with an "Up" link below the root.
fn listing(env: &Env, storage: &str, root: &FileNode, path: &str) -> Element {
    let Some(node) = root.node_at(path) else {
        return empty(env.t.t("api_error_file_not_found"));
    };
    let mut section = div();
    if !path.is_empty() {
        section = section.child(
            nav(env, &browse_at(env, storage, &parent_dir(path)))
                .attr("id", "up")
                .class("wh-back")
                .text(format!("↑ {}", env.t.t("ui_files_up"))),
        );
    }
    if node.children.is_empty() {
        return section.child(empty(env.t.t("ui_files_empty_dir")).attr("id", "empty-dir"));
    }
    let mut list = ul().class("wh-files");
    for (name, child) in node.sorted_children() {
        let child_path = if path.is_empty() {
            name.clone()
        } else {
            format!("{path}/{name}")
        };
        let size = match (child.is_file, child.size) {
            (true, Some(bytes)) => format_bytes(bytes),
            _ => String::new(),
        };
        list = list.child(
            li().class("wh-file-row")
                .child(
                    span()
                        .class("wh-faint")
                        .text(if child.is_file { "▫" } else { "▸" }),
                )
                .child(
                    nav(env, &browse_at(env, storage, &child_path))
                        .class("wh-file-name")
                        .attr("data-entry", child_path.clone())
                        .text(name.clone()),
                )
                .child(span().class("wh-file-size wh-mono").text(size)),
        );
    }
    section.child(list)
}

fn file_detail(
    env: &Env,
    state: &State,
    storage: &str,
    path: &str,
    size: Option<i64>,
    manage: bool,
) -> Element {
    let name = last_segment(path);
    let size_text = size.map_or_else(|| "—".to_string(), format_bytes);
    let meta_list = div()
        .class("wh-meta-list")
        .child(meta_text(env.t.t("ui_files_col_name"), name))
        .child(meta_text(env.t.t("ui_files_col_path"), path))
        .child(meta_text(env.t.t("ui_files_col_type"), content_type(path)))
        .child(meta_text(env.t.t("ui_files_col_size"), &size_text));

    let file_url = |extra: &str| {
        format!(
            "{}/api/v1/files/{}/file?path={}{extra}",
            env.base,
            encode(storage),
            encode(path)
        )
    };
    let mut actions = div().class("wh-actions-row").child(
        a().attr("id", "download")
            .class("wh-btn")
            .attr("href", file_url(""))
            .attr("download", name.to_string())
            .text(env.t.t("ui_file_download")),
    );
    if manage {
        let (state, storage, path) = (state.clone(), storage.to_string(), path.to_string());
        actions = actions.child(
            button()
                .attr("type", "button")
                .attr("id", "delete-file")
                .class("wh-btn wh-danger")
                .on_click(move || {
                    confirm(
                        &state,
                        "ui_modal_delete_title",
                        "ui_file_delete_confirm_text",
                        format!("{storage}/{path}"),
                        Request::DeleteFile {
                            storage: storage.clone(),
                            path: path.clone(),
                        },
                    );
                })
                .text(env.t.t("ui_file_delete")),
        );
    }
    div()
        .child(meta_list)
        .child(preview(env, &file_url("&disposition=inline"), name, path))
        .child(actions)
}

/// The file shown where it is, by what its extension says it is.
fn preview(env: &Env, url: &str, name: &str, path: &str) -> Element {
    let media = match preview_kind(path) {
        PreviewKind::Image => element("img")
            .class("wh-preview-media")
            .attr("src", url.to_string())
            .attr("alt", name.to_string())
            .attr("loading", "lazy"),
        PreviewKind::Video => element("video")
            .class("wh-preview-media")
            .attr("src", url.to_string())
            .attr("controls", "controls")
            .attr("preload", "metadata"),
        PreviewKind::Audio => element("audio")
            .class("wh-preview-audio")
            .attr("src", url.to_string())
            .attr("controls", "controls")
            .attr("preload", "metadata"),
        PreviewKind::Pdf | PreviewKind::Text => element("iframe")
            .class("wh-preview-frame")
            .attr("src", url.to_string())
            .attr("title", name.to_string()),
        PreviewKind::None => {
            return div()
                .class("wh-preview")
                .attr("id", "no-preview")
                .child(empty(env.t.t("ui_file_preview_none")));
        }
    };
    div().class("wh-preview").child(media)
}
