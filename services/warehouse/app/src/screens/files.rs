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
    nav, panel_title, share_name, split, waiting,
};
use crate::live::{Request, State};

fn can_manage(state: &State) -> bool {
    state
        .overview
        .with(|o| o.as_ref().is_some_and(|o| o.can_manage))
}

// ── Storages ─────────────────────────────────────────────────────────────────

#[must_use]
pub fn storages(env: &Env, state: &State) -> View {
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

/// One panel: a bar with the path and the actions, and the directory's entries below it. A file opens in its own window.
#[must_use]
pub fn browse(env: &Env, state: &State) -> View {
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
                untrack(|| {
                    env.ask(Request::Shares(storage.clone()));
                    env.ask(Request::Browse(storage));
                });
            }
        });
    }
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
    let (env, state) = (env.clone(), state.clone());
    View::dynamic(move || {
        let storage = env.query("storage");
        let path = normalize_path(&env.query("path"));
        let manage = can_manage(&state);
        let previewing = env.query("mode") == "preview";
        tree.with(|tree| {
            if previewing
                && let Some((_, _, root, _)) = tree
                    .as_ref()
                    .filter(|t| t.0 == storage && t.1 == Browse::READABLE)
                && let Selection::File { path, .. } = root.resolve(&path)
            {
                return preview_mode(&env, &storage, root, &path);
            }
            let body = match tree.as_ref().filter(|t| t.0 == storage) {
                None => return div().class("wh-fm").child(waiting(&env)).into_view(),
                Some((_, access, _, _)) if access != Browse::READABLE => {
                    empty(access_text(&env, access)).into_view()
                }
                Some((_, _, root, truncated)) => {
                    // A file's address shows the directory that holds it.
                    let dir = match root.resolve(&path) {
                        Selection::Missing(_) => None,
                        Selection::Dir(dir) => Some(dir),
                        Selection::File { path, .. } => Some(parent_dir(&path)),
                    };
                    match dir {
                        None => empty(env.t.t("api_error_file_not_found"))
                            .attr("id", "file-missing")
                            .into_view(),
                        Some(dir) => {
                            listing(&env, &state, &storage, root, &dir, manage, *truncated)
                        }
                    }
                }
            };
            let here = root_dir(tree.as_ref(), &path);
            div()
                .class("wh-fm")
                .child(toolbar(&env, &storage, &here))
                .child(
                    div()
                        .class("wh-fm-body")
                        .child(body)
                        .child(shares_panel(&env, &state, &storage)),
                )
                .into_view()
        })
    })
}

/// The directory the listing shows for `path` (a file's is its parent), as far as it can be told before the tree is loaded.
fn root_dir(tree: Option<&(String, String, FileNode, bool)>, path: &str) -> String {
    match tree.map(|t| t.2.resolve(path)) {
        Some(Selection::File { path, .. }) => parent_dir(&path),
        _ => path.to_string(),
    }
}

/// The top bar: where we are (storage > dir > dir) on the left, the actions on the right.
fn toolbar(env: &Env, storage: &str, dir: &str) -> Element {
    let mut crumbs = div().class("wh-crumbs").child(
        nav(env, &browse_at(env, storage, ""))
            .attr("data-crumb", "")
            .text(storage.to_string()),
    );
    let mut so_far = String::new();
    for segment in dir.split('/').filter(|s| !s.is_empty()) {
        so_far = if so_far.is_empty() {
            segment.to_string()
        } else {
            format!("{so_far}/{segment}")
        };
        crumbs = crumbs.child(span().class("wh-faint").text(">")).child(
            nav(env, &browse_at(env, storage, &so_far))
                .attr("data-crumb", so_far.clone())
                .text(segment.to_string()),
        );
    }
    let mut actions = div().class("wh-fm-actions");
    if !dir.is_empty() {
        actions = actions.child(
            nav(env, &browse_at(env, storage, &parent_dir(dir)))
                .attr("id", "up")
                .class("wh-btn")
                .text(format!("↑ {}", env.t.t("ui_files_up"))),
        );
    }
    actions = actions.child(
        nav(
            env,
            &format!(
                "{}?storage={}",
                env.path("/files/storages"),
                encode(storage)
            ),
        )
        .attr("id", "back")
        .class("wh-btn")
        .text(env.t.t("ui_nav_files")),
    );
    div().class("wh-fm-bar").child(crumbs).child(actions)
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

fn preview_at(env: &Env, storage: &str, path: &str) -> String {
    format!(
        "{}?storage={}&path={}",
        env.path("/files/preview"),
        encode(storage),
        encode(path)
    )
}

fn file_url(env: &Env, storage: &str, path: &str, extra: &str) -> String {
    format!(
        "{}/api/v1/files/{}/file?path={}{extra}",
        env.base,
        encode(storage),
        encode(path)
    )
}

/// The directory's entries, directories first: name, type, size, and what can be done to a file.
#[allow(
    clippy::too_many_lines,
    reason = "one function builds every row of the listing, each with its actions"
)]
fn listing(
    env: &Env,
    state: &State,
    storage: &str,
    root: &FileNode,
    path: &str,
    manage: bool,
    truncated: bool,
) -> View {
    let Some(node) = root.node_at(path) else {
        return empty(env.t.t("api_error_file_not_found")).into_view();
    };
    if node.children.is_empty() {
        return empty(env.t.t("ui_files_empty_dir"))
            .attr("id", "empty-dir")
            .into_view();
    }
    let mut list = div().class("wh-fm-list").child(
        div()
            .class("wh-fm-row wh-fm-head")
            .child(span())
            .child(span().text(env.t.t("ui_files_col_name")))
            .child(span().text(env.t.t("ui_files_col_type")))
            .child(span().class("wh-fm-num").text(env.t.t("ui_files_col_size")))
            .child(span()),
    );
    let can_share = state.browse.with(|b| {
        b.as_ref()
            .is_some_and(|b| b.storage == storage && b.can_share)
    });
    for (name, child) in node.sorted_children() {
        let child_path = if path.is_empty() {
            name.clone()
        } else {
            format!("{path}/{name}")
        };
        let mut row = div().class("wh-fm-row");
        if child.is_file {
            let size = child.size.map_or_else(String::new, format_bytes);
            let mut actions = div().class("wh-fm-row-actions").child(
                a().attr("data-download", child_path.clone())
                    .class("wh-fm-act")
                    .attr("href", file_url(env, storage, &child_path, ""))
                    .attr("download", name.clone())
                    .attr("title", env.t.t("ui_file_download"))
                    .text("⤓"),
            );
            if can_share {
                actions = actions.child(share_button(env, storage, &child_path));
            }
            if manage {
                let (state, storage, target) =
                    (state.clone(), storage.to_string(), child_path.clone());
                actions = actions.child(
                    button()
                        .attr("type", "button")
                        .attr("data-delete", target.clone())
                        .class("wh-fm-act wh-fm-del")
                        .attr("title", env.t.t("ui_file_delete"))
                        .on_click(move || {
                            confirm(
                                &state,
                                "ui_modal_delete_title",
                                "ui_file_delete_confirm_text",
                                format!("{storage}/{target}"),
                                Request::DeleteFile {
                                    storage: storage.clone(),
                                    path: target.clone(),
                                },
                            );
                        })
                        .text("✕"),
                );
            }
            row = row
                .child(span().class("wh-faint").text("▫"))
                .child(
                    nav(env, &preview_mode_at(env, storage, &child_path))
                        .class("wh-file-name")
                        .attr("data-entry", child_path.clone())
                        .text(name.clone()),
                )
                .child(
                    span()
                        .class("wh-faint wh-mono")
                        .text(content_type(&child_path)),
                )
                .child(span().class("wh-fm-num wh-mono").text(size))
                .child(actions);
        } else {
            row = row
                .child(span().class("wh-faint").text("▸"))
                .child(
                    nav(env, &browse_at(env, storage, &child_path))
                        .class("wh-file-name")
                        .attr("data-entry", child_path.clone())
                        .text(name.clone()),
                )
                .child(span().class("wh-faint").text("—"))
                .child(span().class("wh-fm-num"))
                .child(
                    div()
                        .class("wh-fm-row-actions")
                        .child(can_share.then(|| share_button(env, storage, &child_path))),
                );
        }
        list = list.child(row);
    }
    let mut out = div().child(list);
    if truncated {
        out = out.child(
            div()
                .class("wh-truncated")
                .attr("id", "truncated")
                .text(env.t.t("ui_storage_files_truncated")),
        );
    }
    out.into_view()
}

/// The share button's glyph. Plain text like the download (⤓) and delete (✕) beside it, so it takes the button's colour and
/// size; the selector keeps a platform from drawing it as a colour emoji.
const SHARE_GLYPH: &str = "\u{2197}\u{FE0E}";

/// Makes (or fetches again) the share link of one file or directory, and opens the dialog that shows it.
fn share_button(env: &Env, storage: &str, path: &str) -> Element {
    let (env, storage, target) = (env.clone(), storage.to_string(), path.to_string());
    let title = env.t.t("ui_share_button");
    button()
        .attr("type", "button")
        .attr("data-share", target.clone())
        .class("wh-fm-act")
        .attr("title", title)
        .on_click(move || {
            env.ask(Request::CreateShare {
                storage: storage.clone(),
                path: target.clone(),
            });
        })
        .text(SHARE_GLYPH)
}

/// The links already made for this storage that the person may see: what each opens, who made it, and the ways to show or
/// withdraw it. Nothing at all while there are none.
fn shares_panel(env: &Env, state: &State, storage: &str) -> View {
    let (env, state, storage) = (env.clone(), state.clone(), storage.to_string());
    View::dynamic(move || {
        let list = state.shares.with(|shares| match shares {
            Some((name, list)) if *name == storage => list.clone(),
            _ => Vec::new(),
        });
        if list.is_empty() {
            return View::Empty;
        }
        let mut rows = div().class("wh-fm-list").attr("id", "shares");
        for share in list {
            let name = share_name(&share);
            let (open, withdraw) = (share.clone(), share.clone());
            let (shown, ask_state) = (state.share_dialog.clone(), state.clone());
            rows = rows.child(
                div()
                    .class("wh-fm-row")
                    .attr("data-share-id", share.id.clone())
                    .child(
                        span()
                            .class("wh-faint")
                            .text(if share.is_dir { "▸" } else { "▫" }),
                    )
                    .child(span().class("wh-file-name").text(name.clone()))
                    .child(
                        span()
                            .class("wh-faint")
                            .text(format!("{} · {}", share.created_by, share.created)),
                    )
                    .child(span().class("wh-fm-num"))
                    .child(
                        div()
                            .class("wh-fm-row-actions")
                            .child(
                                button()
                                    .attr("type", "button")
                                    .attr("data-share-show", share.id.clone())
                                    .class("wh-fm-act")
                                    .attr("title", env.t.t("ui_share_link"))
                                    .on_click(move || shown.set(Some(open.clone())))
                                    .text(SHARE_GLYPH),
                            )
                            .child(
                                button()
                                    .attr("type", "button")
                                    .attr("data-share-withdraw", share.id.clone())
                                    .class("wh-fm-act wh-fm-del")
                                    .attr("title", env.t.t("ui_share_withdraw"))
                                    .on_click(move || {
                                        confirm(
                                            &ask_state,
                                            "ui_share_withdraw_title",
                                            "ui_share_withdraw_text",
                                            share_name(&withdraw),
                                            Request::DeleteShare {
                                                id: withdraw.id.clone(),
                                                storage: withdraw.storage.clone(),
                                            },
                                        );
                                    })
                                    .text("✕"),
                            ),
                    ),
            );
        }
        div()
            .class("wh-shares")
            .child(panel_title(env.t.t("ui_shares_title")))
            .child(rows)
            .into_view()
    })
}

fn preview_mode_at(env: &Env, storage: &str, path: &str) -> String {
    format!("{}&mode=preview", browse_at(env, storage, path))
}

/// The files of directory `dir`, as paths, in listing order.
fn siblings_of(root: &FileNode, dir: &str) -> Vec<String> {
    root.node_at(dir)
        .map(|n| {
            n.sorted_children()
                .into_iter()
                .filter(|(_, c)| c.is_file)
                .map(|(n, _)| {
                    if dir.is_empty() {
                        n.clone()
                    } else {
                        format!("{dir}/{n}")
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

fn preview_bar(env: &Env, storage: &str, root: &FileNode, path: &str) -> Element {
    let name = last_segment(path);
    let dir = parent_dir(path);
    let siblings = siblings_of(root, &dir);
    let at = siblings.iter().position(|p| p == path).unwrap_or(0);
    let step = |id: &'static str, key: &str, target: Option<&String>| -> Element {
        let label = env.t.t(key);
        match target {
            Some(to) => nav(env, &preview_mode_at(env, storage, to))
                .attr("id", id)
                .attr("data-shortcut", id)
                .class("wh-btn")
                .attr("title", label.clone())
                .text(label),
            None => button()
                .attr("type", "button")
                .attr("id", id)
                .attr("disabled", "disabled")
                .class("wh-btn")
                .text(label),
        }
    };
    let prev = step(
        "prev",
        "ui_preview_prev",
        at.checked_sub(1).and_then(|i| siblings.get(i)),
    );
    let next = step("next", "ui_preview_next", siblings.get(at + 1));

    // Zoom and drag are the page script's (`data-canvas`): the buttons only name what they do. Images only.
    let zoom_button = |id: &'static str, how: &'static str, key: &str, text: &str| {
        button()
            .attr("type", "button")
            .attr("id", id)
            .attr("data-canvas-zoom", how)
            .class("wh-btn")
            .attr("title", env.t.t(key))
            .text(text.to_string())
    };
    let zoom = matches!(preview_kind(path), PreviewKind::Image).then(|| {
        div()
            .class("wh-fm-actions")
            .child(zoom_button("zoom-out", "out", "ui_preview_zoom_out", "−"))
            .child(
                span()
                    .attr("id", "zoom-level")
                    .attr("data-canvas-level", "")
                    .class("wh-faint wh-mono wh-fm-zoom")
                    .text("100%"),
            )
            .child(zoom_button("zoom-in", "in", "ui_preview_zoom_in", "+"))
            .child(zoom_button(
                "zoom-reset",
                "reset",
                "ui_preview_zoom_reset",
                "1:1",
            ))
    });
    div()
        .class("wh-fm-bar")
        .child(
            div()
                .class("wh-fm-actions")
                .child(
                    nav(env, &browse_at(env, storage, &dir))
                        .attr("id", "leave-preview")
                        .class("wh-btn")
                        .text(format!("← {}", env.t.t("ui_preview_back"))),
                )
                .child(span().attr("id", "preview-name").text(name.to_string()))
                .child(span().class("wh-faint wh-mono").text(content_type(path))),
        )
        .child(
            div()
                .class("wh-fm-actions")
                .child(prev)
                .child(
                    span()
                        .class("wh-faint")
                        .text(format!("{} / {}", at + 1, siblings.len())),
                )
                .child(next)
                .child(zoom)
                .child(
                    a().attr("id", "open-window")
                        .class("wh-btn")
                        .attr("href", preview_at(env, storage, path))
                        .attr("target", "_blank")
                        .attr("rel", "noopener")
                        .text(env.t.t("ui_preview_window")),
                )
                .child(
                    a().attr("id", "download")
                        .class("wh-btn")
                        .attr("href", file_url(env, storage, path, ""))
                        .attr("download", name.to_string())
                        .text(env.t.t("ui_file_download")),
                ),
        )
}

fn preview_mode(env: &Env, storage: &str, root: &FileNode, path: &str) -> View {
    let name = last_segment(path);
    let bar = preview_bar(env, storage, root, path);
    let mut tree = ul().class("wh-tree");
    for (child, node) in root.sorted_children() {
        tree = tree.child(tree_node(env, storage, child, node, "", path));
    }
    let mut stage = div().class("wh-fm-stage").child(preview(
        env,
        &file_url(env, storage, path, "&disposition=inline"),
        name,
        path,
    ));
    if matches!(preview_kind(path), PreviewKind::Image) {
        stage = stage.attr("data-canvas", "").class("wh-canvas");
    }
    div()
        .class("wh-fm")
        .child(bar)
        .child(
            div()
                .class("wh-fm-split")
                .child(div().class("wh-fm-tree").child(tree))
                .child(stage),
        )
        .into_view()
}

/// A node of the file tree: a directory (open along the way to the shown file) or a file that opens in the preview.
fn tree_node(
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
    if node.is_leaf() {
        return li().child(
            nav(env, &preview_mode_at(env, storage, &full))
                .class(if selected == full {
                    "wh-link active"
                } else {
                    "wh-link"
                })
                .attr("data-tree-file", full.clone())
                .text(name.to_string()),
        );
    }
    let mut details = element("details").attr("data-path", full.clone());
    if selected.starts_with(&format!("{full}/")) {
        details = details.attr("open", "open");
    }
    let mut list = ul().class("wh-tree");
    for (child, child_node) in node.sorted_children() {
        list = list.child(tree_node(env, storage, child, child_node, &full, selected));
    }
    li().child(
        details
            .child(element("summary").class("wh-folder").text(name.to_string()))
            .child(list),
    )
}

// ── The file preview ─────────────────────────────────────────────────────────

/// A file on its own, for a window of its own: its name and a download in a slim bar, the file filling the rest.
#[must_use]
pub fn preview_page(env: &Env, _state: &State) -> View {
    let storage = env.query("storage");
    let path = normalize_path(&env.query("path"));
    if storage.is_empty() || path.is_empty() {
        return empty(env.t.t("api_error_file_not_found"))
            .attr("id", "file-missing")
            .into_view();
    }
    let name = last_segment(&path);
    let bar = div()
        .class("wh-fm-bar")
        .child(
            div()
                .class("wh-crumbs")
                .child(span().attr("id", "preview-name").text(name.to_string()))
                .child(span().class("wh-faint wh-mono").text(content_type(&path))),
        )
        .child(
            div().class("wh-fm-actions").child(
                a().attr("id", "download")
                    .class("wh-btn")
                    .attr("href", file_url(env, &storage, &path, ""))
                    .attr("download", name.to_string())
                    .text(env.t.t("ui_file_download")),
            ),
        );
    div()
        .class("wh-fm wh-fm-solo")
        .child(bar)
        .child(div().class("wh-fm-stage").child(preview(
            env,
            &file_url(env, &storage, &path, "&disposition=inline"),
            name,
            &path,
        )))
        .into_view()
}

/// The file shown by what its extension says it is.
fn preview(env: &Env, url: &str, name: &str, path: &str) -> Element {
    let media = match preview_kind(path) {
        PreviewKind::Image => element("img")
            .class("wh-preview-media")
            .attr("src", url.to_string())
            .attr("alt", name.to_string()),
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
