use crate::clients::switchboard::SwitchboardClient;
use crate::clients::vllm::VllmClient;
use crate::domain::models::{Conversation, File};
use crate::files::{STATUS_FAILED, STATUS_PROCESSING, STATUS_READY, STATUS_UPLOADED, pipeline};
use crate::routers::files::{create_uploaded_file, parse_upload_form};
use crate::routers::ui::common::RequiredClaims;
use chrono::Utc;
use quench_db::prelude::{Crud, Db};
use quench_http::prelude::{Inject, Multipart, Path, Response, get, http::StatusCode, post};
use quench_starter::common::format::human_bytes;
use quench_starter::common::routes::with_base_path;
use quench_web::prelude::*;

/// Short label for a file's processing status.
fn status_label(status: &str) -> &str {
    match status {
        STATUS_READY => "ready",
        STATUS_PROCESSING => "processing",
        STATUS_UPLOADED => "queued",
        STATUS_FAILED => "failed",
        other => other,
    }
}

/// A compact file chip. `staged` chips self-poll while processing and carry
/// remove/retry buttons; non-staged chips are read-only, under a sent message.
pub fn render_attachment_chip(file: &File, staged: bool) -> Element {
    // No hidden `file_ids` input: `serde_urlencoded` can't deserialize repeated keys.
    let in_progress = file.status == STATUS_UPLOADED || file.status == STATUS_PROCESSING;

    let mut chip = div()
        .class(format!("attachment-chip attachment-chip-{}", file.status))
        .attr("id", format!("chip-{}", file.id))
        .attr("data-file-id", &file.id)
        .attr(
            "title",
            format!("{} · {}", file.file_name, human_bytes(file.file_size)),
        );

    // Polls so the badge updates queued -> processing -> ready/failed live.
    if staged && in_progress {
        chip = chip
            .attr(
                "hx-get",
                with_base_path(&format!("/ui/files/chip/{}", file.id)),
            )
            .attr("hx-trigger", "every 2s")
            // Without this, htmx inherits hx-target from the chat form and swaps the wrong node.
            .attr("hx-target", "this")
            .attr("hx-swap", "outerHTML");
    }

    // Images show a real thumbnail instead of the generic file icon.
    let icon = if crate::files::is_image_mime(&file.mime_type) {
        element("img")
            .class("attachment-thumb")
            .attr(
                "src",
                with_base_path(&format!("/api/v1/files/{}/download", file.id)),
            )
            .attr("alt", &file.file_name)
            .attr("loading", "lazy")
    } else {
        i().class("fas fa-file-lines attachment-icon")
    };
    chip = chip
        .child(icon)
        .child(span().class("attachment-name").text(&file.file_name))
        .child(
            span()
                .class("attachment-size")
                .text(human_bytes(file.file_size)),
        );

    // Status badge: the data-i18n key follows the raw status, with the English text as fallback.
    let mut badge = span()
        .class(format!(
            "attachment-status attachment-status-{}",
            file.status
        ))
        .attr("data-i18n", format!("ui_file_status_{}", file.status))
        .text(status_label(&file.status));
    if file.status == STATUS_FAILED
        && let Some(err) = &file.error_message
    {
        badge = badge.attr("title", err.clone());
    }
    chip = chip.child(badge);

    if staged {
        if file.status == STATUS_FAILED {
            chip = chip.child(
                button()
                    .attr("type", "button")
                    .class("attachment-retry")
                    .attr("title", "Retry processing")
                    .attr("data-i18n-title", "ui_files_retry_tooltip")
                    .attr(
                        "hx-post",
                        with_base_path(&format!("/ui/files/reprocess/{}", file.id)),
                    )
                    .attr("hx-target", format!("#chip-{}", file.id))
                    .attr("hx-swap", "outerHTML")
                    .child(i().class("fas fa-rotate-right")),
            );
        }
        chip = chip.child(
            button()
                .attr("type", "button")
                .class("attachment-remove")
                .attr("title", "Cancel / remove")
                .attr("data-i18n-title", "ui_files_remove_tooltip")
                .attr(
                    "hx-post",
                    with_base_path(&format!("/ui/files/detach/{}", file.id)),
                )
                .attr("hx-target", format!("#chip-{}", file.id))
                .attr("hx-swap", "outerHTML")
                .child(i().class("fas fa-xmark")),
        );
    } else {
        chip = chip.child(
            a().class("attachment-download")
                .attr("title", "Download")
                .attr("data-i18n-title", "ui_files_download_tooltip")
                .attr(
                    "href",
                    with_base_path(&format!("/api/v1/files/{}/download", file.id)),
                )
                .child(i().class("fas fa-download")),
        );
    }

    chip
}

/// A file row in a project's sidebar: name links to download, menu offers deletion.
pub fn render_project_file_row(file: &File) -> Element {
    let item_id = format!("file-item-{}", file.id);

    let mut name_link = a()
        .class("history-item-link file-item-link")
        .attr(
            "href",
            with_base_path(&format!("/api/v1/files/{}/download", file.id)),
        )
        .attr(
            "title",
            format!(
                "{} · {} · {}",
                file.file_name,
                human_bytes(file.file_size),
                status_label(&file.status)
            ),
        )
        .child(i().class(if crate::files::is_image_mime(&file.mime_type) {
            "fas fa-image file-item-icon"
        } else {
            "fas fa-file-lines file-item-icon"
        }))
        .child(span().class("file-item-name").text(&file.file_name));

    // Surface a status badge for anything not yet searchable, so processing/failed files are visible at a glance.
    if file.status != STATUS_READY {
        name_link = name_link.child(
            span()
                .class(format!(
                    "attachment-status attachment-status-{}",
                    file.status
                ))
                .attr("data-i18n", format!("ui_file_status_{}", file.status))
                .text(status_label(&file.status)),
        );
    }

    div()
        .class("history-item project-file-item")
        .attr("id", &item_id)
        .child(name_link)
        .child(
            div()
                .class("menu-container")
                .child(
                    button()
                        .class("menu-trigger-btn")
                        .child(i().class("fas fa-ellipsis-v")),
                )
                .child(
                    div().class("dropdown-menu").child(
                        button()
                            .class("dropdown-item delete-item")
                            .attr(
                                "hx-get",
                                with_base_path(&format!("/ui/files/delete-modal/{}", file.id)),
                            )
                            .attr("hx-target", "#confirm-delete-modal")
                            .attr("hx-swap", "outerHTML")
                            .child(i().class("fas fa-trash"))
                            .child(span().attr("data-i18n", "ui_common_delete").text("Delete")),
                    ),
                ),
        )
}

/// The collapsible "Files" sidebar section - header and content as siblings, for the toggle.
pub fn render_project_files_section(files: &[File]) -> Element {
    let header = div()
        .class("history-section-header collapsible files-section-header")
        .attr("onclick", "this.classList.toggle('open'); const content = this.nextElementSibling; if(content) { content.classList.toggle('hidden'); }")
        .child(
            div()
                .attr("style", "display: flex; align-items: center; gap: 0.5rem;")
                .child(i().class("fas fa-chevron-right chevron"))
                .child(i().class("fas fa-folder-tree files-section-icon"))
                .child(span().attr("data-i18n", "ui_sidebar_files").text("Files")),
        )
        .child(span().class("files-section-count").text(files.len().to_string()));

    // Collapsed by default: an unobtrusive "Files ›" the user expands.
    let mut content = div().class("history-section-content hidden");
    if files.is_empty() {
        content = content.child(
            div()
                .class("files-empty")
                .attr("data-i18n", "ui_files_empty_project")
                .text("No files uploaded for this project."),
        );
    } else {
        for file in files {
            content = content.child(render_project_file_row(file));
        }
    }

    div().child(header).child(content)
}

/// A read-only row of attachment chips shown inside a sent user message; `None` if there are none.
pub fn render_attachments_row(files: &[File]) -> Option<Element> {
    if files.is_empty() {
        return None;
    }
    let mut row = div().class("message-attachments");
    for file in files {
        row = row.child(render_attachment_chip(file, false));
    }
    Some(row)
}

/// Load the caller's files by id for rendering chips before they're linked; silently skips ids the user doesn't own.
pub async fn load_owned_files(db: &Db, file_ids: &[String], username: &str) -> Vec<File> {
    let mut files = Vec::new();
    let repo = db.repository::<File>();
    for id in file_ids {
        if let Ok(Some(f)) = repo.read(id).await
            && f.owner == username
        {
            files.push(f);
        }
    }
    files
}

fn html(status: StatusCode, body: impl Into<String>) -> Response {
    Response::html(status, body)
}

/// Uploads a file from the composer, staged (message_id NULL) until the
/// message is sent; creates the conversation row first if it isn't persisted yet.
#[post("/ui/files/attach")]
pub async fn attach(
    claims: RequiredClaims,
    Inject(db): Inject<Db>,
    Inject(switchboard): Inject<SwitchboardClient>,
    Inject(vllm): Inject<VllmClient>,
    form: Multipart,
) -> Response {
    let username = match claims.or_401() {
        Ok(claims) => claims.sub,
        Err(response) => return response,
    };

    let mut form = match parse_upload_form(form).await {
        Ok(form) => form,
        Err(err) => return err.into_response(),
    };
    let Some(conversation_id) = form.conversation_id.clone() else {
        return Response::text(StatusCode::BAD_REQUEST, "api_error_missing_conversation_id");
    };
    let project_id = form.project_id.clone();

    // The conversation may not be persisted yet (fresh chat): create it so the file's conversation_id FK holds.
    let conv_repo = db.repository::<Conversation>();
    match conv_repo.read(&conversation_id).await {
        Ok(Some(c)) if c.owner != username => return Response::new(StatusCode::FORBIDDEN),
        Ok(Some(_)) => {}
        Ok(None) => {
            let now = Utc::now().to_rfc3339();
            let conv = Conversation {
                id: conversation_id.clone(),
                // Blank until the first message is sent (see title logic in stream_message), so that text becomes the title.
                title: String::new(),
                active_message_id: None,
                owner: username.clone(),
                project_id: project_id.clone(),
                updated_at: now,
            };
            if let Err(e) = conv_repo.create(&conv).await {
                tracing::error!("Failed to create conversation for attachment: {}", e);
                return Response::text(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "api_error_conversation_create_failed",
                );
            }
        }
        Err(e) => {
            tracing::error!("Internal error: {}", e);
            return Response::text(StatusCode::INTERNAL_SERVER_ERROR, "api_error_internal");
        }
    }

    // create_uploaded_file expects exactly one scope; attach to the conversation.
    form.project_id = None;

    match create_uploaded_file(&db, &switchboard, &vllm, &username, form).await {
        Ok(file) => html(StatusCode::OK, render_attachment_chip(&file, true).render()),
        Err(resp) => resp,
    }
}

/// Remove a staged (not-yet-sent) attachment owned by the user and not yet linked to a message.
/// Returns an empty body so the chip is swapped out.
#[post("/ui/files/detach/{file_id}")]
pub async fn detach(
    claims: RequiredClaims,
    Inject(db): Inject<Db>,
    Path(file_id): Path<String>,
) -> Response {
    let username = match claims.or_401() {
        Ok(claims) => claims.sub,
        Err(response) => return response,
    };

    let repo = db.repository::<File>();
    match repo.read(&file_id).await {
        // Only staged files (message_id NULL) may be detached this way.
        Ok(Some(f)) if f.owner == username && f.message_id.is_none() => {
            if let Err(e) = repo.delete(&f.id).await {
                tracing::error!("Failed to detach file {}: {}", f.id, e);
            }
        }
        Ok(_) => {}
        Err(e) => tracing::error!("Failed to read file for detach: {}", e),
    }

    // Empty body: htmx swaps the chip out of the staging area.
    html(StatusCode::OK, "")
}

/// Return the current chip for a staged file, polled while processing; an empty body (file gone) removes the chip.
#[get("/ui/files/chip/{file_id}")]
pub async fn chip_status(
    claims: RequiredClaims,
    Inject(db): Inject<Db>,
    Path(file_id): Path<String>,
) -> Response {
    let username = match claims.or_401() {
        Ok(claims) => claims.sub,
        Err(response) => return response,
    };

    match db.repository::<File>().read(&file_id).await {
        Ok(Some(f)) if f.owner == username => {
            html(StatusCode::OK, render_attachment_chip(&f, true).render())
        }
        Ok(Some(_)) => Response::new(StatusCode::FORBIDDEN),
        // File no longer exists: empty body swaps the chip out.
        Ok(None) => html(StatusCode::OK, ""),
        Err(e) => {
            tracing::error!("Internal error: {}", e);
            Response::text(StatusCode::INTERNAL_SERVER_ERROR, "api_error_internal")
        }
    }
}

/// Retry processing a failed staged file; returns a chip in the processing state so the composer resumes polling.
#[post("/ui/files/reprocess/{file_id}")]
pub async fn reprocess(
    claims: RequiredClaims,
    Inject(db): Inject<Db>,
    Inject(switchboard): Inject<SwitchboardClient>,
    Inject(vllm): Inject<VllmClient>,
    Path(file_id): Path<String>,
) -> Response {
    let username = match claims.or_401() {
        Ok(claims) => claims.sub,
        Err(response) => return response,
    };

    match db.repository::<File>().read(&file_id).await {
        // Images have no text pipeline to (re)run; return the chip unchanged.
        Ok(Some(f)) if f.owner == username && crate::files::is_image_mime(&f.mime_type) => {
            html(StatusCode::OK, render_attachment_chip(&f, true).render())
        }
        Ok(Some(mut f)) if f.owner == username => {
            pipeline::spawn_processing(
                (*db).clone(),
                (*switchboard).clone(),
                (*vllm).clone(),
                f.id.clone(),
            );
            // Reflect the imminent state so the returned chip polls again.
            f.status = STATUS_PROCESSING.to_string();
            f.error_message = None;
            html(StatusCode::OK, render_attachment_chip(&f, true).render())
        }
        Ok(Some(_)) => Response::new(StatusCode::FORBIDDEN),
        Ok(None) => Response::text(StatusCode::NOT_FOUND, "api_error_file_not_found"),
        Err(e) => {
            tracing::error!("Internal error: {}", e);
            Response::text(StatusCode::INTERNAL_SERVER_ERROR, "api_error_internal")
        }
    }
}

/// Confirmation modal for deleting a project file from the sidebar, mirroring the conversation delete modal.
#[get("/ui/files/delete-modal/{file_id}")]
pub async fn delete_modal(
    claims: RequiredClaims,
    Inject(db): Inject<Db>,
    Path(file_id): Path<String>,
) -> Response {
    let username = match claims.or_401() {
        Ok(claims) => claims.sub,
        Err(response) => return response,
    };

    let name = match db.repository::<File>().read(&file_id).await {
        Ok(Some(f)) if f.owner == username => format!("\"{}\"", f.file_name),
        Ok(Some(_)) => return Response::new(StatusCode::FORBIDDEN),
        Ok(None) => return Response::text(StatusCode::NOT_FOUND, "api_error_file_not_found"),
        Err(e) => {
            tracing::error!("Internal error: {}", e);
            return Response::text(StatusCode::INTERNAL_SERVER_ERROR, "api_error_internal");
        }
    };

    let modal = div()
        .attr("id", "confirm-delete-modal")
        .class("estimates-modal open")
        .child(
            button()
                .class("estimates-modal-backdrop")
                .attr("type", "button")
                .attr(
                    "hx-get",
                    with_base_path("/ui/chat/conversations/delete-modal/empty"),
                )
                .attr("hx-target", "#confirm-delete-modal")
                .attr("hx-swap", "outerHTML"),
        )
        .child(
            div()
                .class("estimates-modal-content small")
                .child(
                    div()
                        .class("estimates-modal-header")
                        .child(
                            div()
                                .class("estimates-modal-title")
                                .attr("data-i18n", "ui_modal_delete_title")
                                .text("Confirm Delete"),
                        )
                        .child(
                            button()
                                .class("estimates-modal-close")
                                .attr("type", "button")
                                .attr(
                                    "hx-get",
                                    with_base_path("/ui/chat/conversations/delete-modal/empty"),
                                )
                                .attr("hx-target", "#confirm-delete-modal")
                                .attr("hx-swap", "outerHTML")
                                .child(i().class("fas fa-times")),
                        ),
                )
                .child(
                    div()
                        .class("estimates-modal-body")
                        .child(
                            p().attr("data-i18n", "ui_files_delete_confirm_text")
                                .text("Are you sure you want to delete this file?"),
                        )
                        .child(div().class("model-to-delete-name").text(name))
                        .child(
                            form()
                                .class("confirm-actions")
                                .attr(
                                    "hx-post",
                                    with_base_path(&format!("/ui/files/delete/{}", file_id)),
                                )
                                .attr("hx-target", "#confirm-delete-modal")
                                .attr("hx-swap", "outerHTML")
                                .child(
                                    button()
                                        .class("button cancel")
                                        .attr("type", "button")
                                        .attr(
                                            "hx-get",
                                            with_base_path(
                                                "/ui/chat/conversations/delete-modal/empty",
                                            ),
                                        )
                                        .attr("hx-target", "#confirm-delete-modal")
                                        .attr("hx-swap", "outerHTML")
                                        .attr("data-i18n", "ui_common_cancel")
                                        .text("Cancel"),
                                )
                                .child(
                                    button()
                                        .class("button danger")
                                        .attr("type", "submit")
                                        .attr("data-i18n", "ui_common_delete")
                                        .text("Delete"),
                                ),
                        ),
                ),
        );

    html(StatusCode::OK, modal.render())
}

/// Delete a project file from the sidebar; returns a closed modal plus an out-of-band swap
/// that removes the file's row. Blobs and chunks cascade.
#[post("/ui/files/delete/{file_id}")]
pub async fn delete_file_ui(
    claims: RequiredClaims,
    Inject(db): Inject<Db>,
    Path(file_id): Path<String>,
) -> Response {
    let username = match claims.or_401() {
        Ok(claims) => claims.sub,
        Err(response) => return response,
    };

    let repo = db.repository::<File>();
    match repo.read(&file_id).await {
        Ok(Some(f)) if f.owner == username => {
            if let Err(e) = repo.delete(&f.id).await {
                tracing::error!("Failed to delete file {}: {}", f.id, e);
                return Response::text(StatusCode::INTERNAL_SERVER_ERROR, "api_error_internal");
            }
        }
        Ok(Some(_)) => return Response::new(StatusCode::FORBIDDEN),
        Ok(None) => return Response::text(StatusCode::NOT_FOUND, "api_error_file_not_found"),
        Err(e) => {
            tracing::error!("Internal error: {}", e);
            return Response::text(StatusCode::INTERNAL_SERVER_ERROR, "api_error_internal");
        }
    }

    let close_modal = div()
        .attr("id", "confirm-delete-modal")
        .class("estimates-modal")
        .render();
    let oob_delete = div()
        .attr("id", format!("file-item-{}", file_id))
        .attr("hx-swap-oob", "delete")
        .render();

    html(StatusCode::OK, format!("{}{}", close_modal, oob_delete))
}

pub fn register_routes() {
    let _ = attach as fn(_, _, _, _, _) -> _;
    let _ = detach as fn(_, _, _) -> _;
    let _ = chip_status as fn(_, _, _) -> _;
    let _ = reprocess as fn(_, _, _, _, _) -> _;
    let _ = delete_modal as fn(_, _, _) -> _;
    let _ = delete_file_ui as fn(_, _, _) -> _;
}
