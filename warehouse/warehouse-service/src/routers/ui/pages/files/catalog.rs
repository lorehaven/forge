use crate::domain::jwt::JwtConfig;
use crate::routers::files::{list_directory, list_storage_infos};
use crate::routers::ui::common::{
    UiPageKind, is_ui_authenticated, render_page, ui_login_redirect, ui_path,
};
use crate::routers::with_base_path;
use actix_web::{HttpRequest, HttpResponse, Responder, get, web};
use quench_web::prelude::*;
use serde::Deserialize;

#[derive(Deserialize)]
pub(super) struct FilesQuery {
    storage: Option<String>,
    path: Option<String>,
    item: Option<String>,
}

#[get("/files/catalog")]
pub(in crate::routers::ui::pages) async fn files_catalog(
    req: HttpRequest,
    query: web::Query<FilesQuery>,
    config: web::Data<JwtConfig>,
) -> impl Responder {
    if !is_ui_authenticated(&req, &config) {
        return ui_login_redirect();
    }

    render_files_page(
        query.storage.clone(),
        query.path.clone(),
        query.item.clone(),
    )
}

#[get("/files/catalog/")]
pub(in crate::routers::ui::pages) async fn files_catalog_slash(
    req: HttpRequest,
    query: web::Query<FilesQuery>,
    config: web::Data<JwtConfig>,
) -> impl Responder {
    if !is_ui_authenticated(&req, &config) {
        return ui_login_redirect();
    }

    render_files_page(
        query.storage.clone(),
        query.path.clone(),
        query.item.clone(),
    )
}

fn render_files_page(
    selected_storage: Option<String>,
    selected_path: Option<String>,
    selected_item: Option<String>,
) -> HttpResponse {
    let storages = list_storage_infos();
    let storage_name = selected_storage
        .filter(|name| storages.iter().any(|storage| storage.name == *name))
        .or_else(|| storages.first().map(|storage| storage.name.clone()));
    let current_path = selected_path.unwrap_or_default();

    let entries = storage_name
        .as_deref()
        .and_then(|name| list_directory(name, &current_path))
        .unwrap_or_default();
    let selected_entry = selected_item
        .as_deref()
        .and_then(|item| entries.iter().find(|entry| entry.path == item));

    let left = div()
        .class("split-left panel")
        .child(
            div()
                .class("panel-title")
                .attr("data-i18n", "ui_files_storages"),
        )
        .child(render_storage_list(&storages, storage_name.as_deref()));

    let right = div()
        .class("split-right")
        .child(div().class("right-top").child(render_entries_panel(
            storage_name.as_deref(),
            &current_path,
            &entries,
            selected_item.as_deref(),
        )))
        .child(div().class("right-bottom").child(render_metadata_panel(
            storage_name.as_deref(),
            &current_path,
            &entries,
            selected_entry,
        )));

    render_page(
        HttpResponse::Ok(),
        content()
            .class("container-fluid py-4")
            .child(div().class("split-view").child(left).child(right)),
        UiPageKind::Files,
    )
}

fn render_storage_list(
    storages: &[crate::routers::files::FileStorageInfo],
    selected: Option<&str>,
) -> Element {
    if storages.is_empty() {
        return div()
            .class("empty")
            .attr("data-i18n", "ui_files_empty_storages");
    }

    let mut list = ul().class("repo-tree");
    for storage in storages {
        let href = format!(
            "{}?storage={}&path=",
            ui_path("/files/catalog"),
            storage.name
        );
        let class = if Some(storage.name.as_str()) == selected {
            "repo-link active"
        } else {
            "repo-link"
        };

        list = list.child(li().child(a().attr("href", &href).class(class).text(&storage.name)));
    }

    div().class("tree-scroll").child(list)
}

fn render_entries_panel(
    storage: Option<&str>,
    path: &str,
    entries: &[crate::routers::files::DirectoryEntry],
    selected_item: Option<&str>,
) -> Element {
    let title = match storage {
        Some(name) if path.is_empty() => div()
            .class("panel-title")
            .child(span().attr("data-i18n", "ui_files_entries_for"))
            .child(span().text(&format!(" {name} /"))),
        Some(name) => div()
            .class("panel-title")
            .child(span().attr("data-i18n", "ui_files_entries_for"))
            .child(span().text(&format!(" {name} /{path}"))),
        None => div()
            .class("panel-title")
            .attr("data-i18n", "ui_files_entries"),
    };

    let mut toolbar = div().class("files-toolbar");
    if let Some(storage_name) = storage {
        let parent_link = parent_path(path).map(|parent| {
            let href = if parent.is_empty() {
                format!("{}?storage={storage_name}", ui_path("/files/catalog"))
            } else {
                format!(
                    "{}?storage={storage_name}&path={parent}",
                    ui_path("/files/catalog")
                )
            };
            a().attr("href", &href)
                .class("button")
                .attr("data-i18n", "ui_files_up")
        });

        toolbar = toolbar
            .child_opt(parent_link)
            .child(
                input()
                    .attr("id", "current-path")
                    .attr("type", "hidden")
                    .attr("value", path),
            )
            .child(
                input()
                    .attr("id", "upload-input")
                    .attr("type", "file")
                    .attr("multiple", "multiple"),
            )
            .child(
                button()
                    .class("button")
                    .attr("type", "button")
                    .attr("data-i18n", "ui_files_upload")
                    .on_click(&format!("uploadFiles('{storage_name}')")),
            )
            .child(
                a().attr(
                    "href",
                    &format!(
                        "{}/download?path={}",
                        with_base_path(&format!("/api/v1/files/{storage_name}")),
                        url_encode(path)
                    ),
                )
                .class("button")
                .attr("data-i18n", "ui_files_download_folder"),
            )
            .child(
                button()
                    .class("button")
                    .attr("type", "button")
                    .attr("data-i18n", "ui_files_add_folder")
                    .on_click(&format!(
                        "createFolder('{storage_name}', '{}')",
                        js_escape(path)
                    )),
            )
            .child(
                button()
                    .class("button")
                    .attr("type", "button")
                    .attr("data-i18n", "ui_files_bulk_download")
                    .on_click(&format!("bulkDownload('{storage_name}')")),
            )
            .child(
                button()
                    .class("button")
                    .attr("type", "button")
                    .attr("data-i18n", "ui_files_bulk_delete")
                    .on_click(&format!("bulkDelete('{storage_name}')")),
            );
    }

    let header = div()
        .class("header")
        .child(div().class("cell").text(""))
        .child(div().class("cell").attr("data-i18n", "ui_files_col_name"))
        .child(div().class("cell").attr("data-i18n", "ui_files_col_type"))
        .child(div().class("cell").attr("data-i18n", "ui_files_col_size"))
        .child(
            div()
                .class("cell")
                .attr("data-i18n", "ui_files_col_actions"),
        );

    let mut body = div().class("body");
    if storage.is_none() {
        body = body.child(
            div()
                .class("empty")
                .attr("data-i18n", "ui_files_empty_storages"),
        );
    } else if entries.is_empty() {
        body = body.child(div().class("empty").attr("data-i18n", "ui_files_empty_dir"));
    } else {
        let storage_name = storage.unwrap_or_default();
        for entry in entries {
            let item_url = if path.is_empty() {
                format!(
                    "{}?storage={storage_name}&item={}",
                    ui_path("/files/catalog"),
                    url_encode(&entry.path)
                )
            } else {
                format!(
                    "{}?storage={storage_name}&path={}&item={}",
                    ui_path("/files/catalog"),
                    url_encode(path),
                    url_encode(&entry.path)
                )
            };
            let size_label = if entry.is_dir {
                "-".to_string()
            } else {
                entry.size_bytes.to_string()
            };
            let open_link = if entry.is_dir {
                let href = format!(
                    "{}?storage={storage_name}&path={}",
                    ui_path("/files/catalog"),
                    url_encode(&entry.path)
                );
                a().attr("href", &href).class("tag-link").text(&entry.name)
            } else {
                span().class("mono").text(&entry.name)
            };
            let row_class = if Some(entry.path.as_str()) == selected_item {
                "row active"
            } else {
                "row"
            };

            let row = div()
                .class(row_class)
                .on_click(&format!("handleRowSelect(event, '{}')", item_url))
                .child(
                    div().class("cell").child(
                        input()
                            .attr("type", "checkbox")
                            .class("bulk-path")
                            .attr("data-path", &entry.path),
                    ),
                )
                .child(div().class("cell").child(open_link))
                .child(
                    div()
                        .class("cell")
                        .text(if entry.is_dir { "folder" } else { "file" }),
                )
                .child(div().class("cell mono").text(&size_label))
                .child(
                    div()
                        .class("cell actions")
                        .child_opt((!entry.is_dir).then(|| {
                            i().class("fas fa-eye")
                                .attr("aria-hidden", "true")
                                .attr("data-action", "preview-file")
                                .attr("title", "Preview file")
                                .attr("role", "button")
                                .attr("aria-label", "Preview file")
                                .on_click(&format!(
                                    "previewPath('{}', '{}')",
                                    storage_name,
                                    js_escape(&entry.path)
                                ))
                        }))
                        .child(
                            i().class("fas fa-download")
                                .attr("aria-hidden", "true")
                                .attr("data-action", "download-path")
                                .attr(
                                    "title",
                                    if entry.is_dir {
                                        "Download folder as zip"
                                    } else {
                                        "Download file"
                                    },
                                )
                                .attr("role", "button")
                                .attr("aria-label", "Download")
                                .on_click(&format!(
                                    "downloadPath('{}', '{}')",
                                    storage_name,
                                    js_escape(&entry.path)
                                )),
                        )
                        .child(
                            i().class("fas fa-trash")
                                .attr("aria-hidden", "true")
                                .attr("data-action", "delete-path")
                                .attr(
                                    "title",
                                    if entry.is_dir {
                                        "Delete folder"
                                    } else {
                                        "Delete file"
                                    },
                                )
                                .attr("role", "button")
                                .attr("aria-label", "Delete")
                                .on_click(&format!(
                                    "deletePath('{}', '{}', {})",
                                    storage_name,
                                    js_escape(&entry.path),
                                    if entry.is_dir { "true" } else { "false" }
                                )),
                        ),
                );

            body = body.child(row);
        }
    }

    div()
        .class("panel table file-grid")
        .child(title)
        .child(toolbar)
        .child(div().class("table-scroll").child(header).child(body))
}

fn render_metadata_panel(
    storage: Option<&str>,
    path: &str,
    entries: &[crate::routers::files::DirectoryEntry],
    selected: Option<&crate::routers::files::DirectoryEntry>,
) -> Element {
    let title = div()
        .class("panel-title")
        .attr("data-i18n", "ui_files_metadata");
    let body = match storage {
        None => div()
            .class("empty")
            .attr("data-i18n", "ui_files_empty_storages"),
        Some(storage_name) => {
            let file_count = entries.iter().filter(|entry| !entry.is_dir).count();
            let dir_count = entries.iter().filter(|entry| entry.is_dir).count();
            let total_size = entries
                .iter()
                .filter(|entry| !entry.is_dir)
                .map(|entry| entry.size_bytes)
                .sum::<u64>();

            let mut body = div()
                .class("meta-list")
                .child(meta_row("Storage", storage_name))
                .child(meta_row(
                    "Directory",
                    if path.is_empty() { "/" } else { path },
                ))
                .child(meta_row("Directories", &dir_count.to_string()))
                .child(meta_row("Files", &file_count.to_string()))
                .child(meta_row(
                    "File Bytes (current dir)",
                    &total_size.to_string(),
                ));

            if let Some(entry) = selected {
                let selected_size = if entry.is_dir {
                    "-".to_string()
                } else {
                    entry.size_bytes.to_string()
                };
                body = body
                    .child(meta_row("Selected", &entry.name))
                    .child(meta_row("Selected Path", &entry.path))
                    .child(meta_row(
                        "Selected Type",
                        if entry.is_dir { "folder" } else { "file" },
                    ))
                    .child(meta_row("Selected Size", &selected_size))
                    .child(meta_row(
                        "Available Actions",
                        if entry.is_dir {
                            "download zip, delete"
                        } else {
                            "preview, download, delete"
                        },
                    ));
            } else {
                body = body.child(meta_row(
                    "Selected",
                    "none (click a file name to inspect details)",
                ));
            }
            body
        }
    };
    div().class("panel").child(title).child(body)
}

fn meta_row(label: &str, value: &str) -> Element {
    div()
        .class("meta-row")
        .child(div().class("meta-label").text(label))
        .child(div().class("meta-value mono").text(value))
}

fn parent_path(path: &str) -> Option<String> {
    if path.is_empty() {
        return None;
    }
    let mut parts: Vec<&str> = path.split('/').collect();
    parts.pop();
    Some(parts.join("/"))
}

fn url_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for b in value.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(b as char);
            }
            b' ' => out.push_str("%20"),
            _ => {
                out.push('%');
                out.push_str(&format!("{b:02X}"));
            }
        }
    }
    out
}

fn js_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('\'', "\\'")
}
