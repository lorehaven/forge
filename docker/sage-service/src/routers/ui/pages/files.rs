use crate::clients::switchboard::SwitchboardClient;
use crate::clients::vllm::VllmClient;
use crate::files::{STATUS_FAILED, STATUS_PROCESSING, STATUS_READY, STATUS_UPLOADED, pipeline};
use crate::models::{Conversation, File};
use crate::routers::files::{FileUploadForm, create_uploaded_file};
use actix_multipart::form::MultipartForm;
use actix_web::{HttpResponse, Responder, get, post, web};
use chrono::Utc;
use quench_auth::actix::routers::ui::get_user_from_req;
use quench_auth::prelude::JwtConfig;
use quench_db::prelude::{Crud, Db};
use quench_starter::prelude::with_base_path;
use quench_web::prelude::*;

/// Human-readable byte size for a chip label.
fn format_size(bytes: i64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    let b = bytes as f64;
    if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.0} KB", b / KB)
    } else {
        format!("{} B", bytes)
    }
}

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

/// A compact file chip. `staged` chips (in the composer, above the prompt)
/// carry a `data-file-id` for form submission, a remove/cancel button, a retry
/// on failure, and self-poll while still processing so the status badge stays
/// live. Non-staged chips are read-only attachments shown under a sent message.
pub fn render_attachment_chip(file: &File, staged: bool) -> Element {
    // `data-file-id` lets the composer collect staged ids at submit time (see
    // the chat form's htmx:config-request handler). We deliberately avoid a
    // hidden `file_ids` input: actix's urlencoded form parser (serde_urlencoded)
    // cannot deserialize repeated keys into a Vec and errors on the whole form.
    let in_progress = file.status == STATUS_UPLOADED || file.status == STATUS_PROCESSING;

    let mut chip = div()
        .class(format!("attachment-chip attachment-chip-{}", file.status))
        .attr("id", format!("chip-{}", file.id))
        .attr("data-file-id", &file.id)
        .attr(
            "title",
            format!("{} · {}", file.file_name, format_size(file.file_size)),
        );

    // Poll for status while the file is still being extracted/embedded, so the
    // badge updates from queued → processing → ready/failed without a reload.
    if staged && in_progress {
        chip = chip
            .attr(
                "hx-get",
                with_base_path(&format!("/ui/files/chip/{}", file.id)),
            )
            .attr("hx-trigger", "every 2s")
            // Pin target to this chip: without it htmx inherits hx-target from
            // the enclosing chat form (.chat-history) and swaps the wrong node.
            .attr("hx-target", "this")
            .attr("hx-swap", "outerHTML");
    }

    chip = chip
        .child(i().class("fas fa-file-lines attachment-icon"))
        .child(span().class("attachment-name").text(&file.file_name))
        .child(
            span()
                .class("attachment-size")
                .text(format_size(file.file_size)),
        );

    // Status badge. The data-i18n key follows the raw status so every state
    // resolves to a localized label; the English text stays as fallback.
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

/// A single file row in a project's sidebar "Files" section. The name is a
/// download link (cookie auth works on the API scope) and the three-dot menu
/// offers deletion, mirroring the conversation history rows.
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
                format_size(file.file_size),
                status_label(&file.status)
            ),
        )
        .child(i().class("fas fa-file-lines file-item-icon"))
        .child(span().class("file-item-name").text(&file.file_name));

    // Surface a status badge for anything not yet searchable so the user can
    // see processing / failed files at a glance.
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

/// The collapsible "Files" section for a project's sidebar, listing every file
/// visible to the project (attached to it or to any of its conversations).
/// Returns a container holding the header and its (collapsed) content so the
/// header's `nextElementSibling` toggle finds the content.
pub fn render_project_files_section(files: &[File]) -> Element {
    let header = div()
        .class("history-section-header collapsible files-section-header")
        .attr("onclick", "this.classList.toggle('open'); const content = this.nextElementSibling; if(content) { content.classList.toggle('hidden'); }")
        .child(
            div()
                .attr("style", "display: flex; align-items: center; gap: 0.5rem;")
                .child(i().class("fas fa-chevron-right chevron"))
                .child(i().class("fas fa-folder-tree files-section-icon"))
                .child(span().attr("data-i18n", "ui_sidebar_files").text("Files"))
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

/// A read-only row of attachment chips shown inside a sent user message.
/// Returns None when there are no attachments.
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

/// Load the caller's files by id (used to render chips for a message being
/// sent, before they are linked). Silently skips ids the user does not own.
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

/// Upload a file from the chat composer. Ensures the (possibly not-yet-sent)
/// conversation row exists so the file's FK is valid, stores the file staged
/// (message_id NULL), and returns a chip to append to the composer's staging
/// area. The chip is linked to the user message when the message is sent.
#[post("/attach")]
pub async fn attach(
    req: actix_web::HttpRequest,
    jwt_config: web::Data<JwtConfig>,
    db: web::Data<Db>,
    switchboard: web::Data<SwitchboardClient>,
    vllm: web::Data<VllmClient>,
    form: MultipartForm<FileUploadForm>,
) -> impl Responder {
    let username = match get_user_from_req(&req, &jwt_config).await {
        Some(claims) => claims.sub,
        None => return HttpResponse::Unauthorized().finish(),
    };

    let mut form = form.into_inner();
    let Some(conversation_id) = form.conversation_id.as_ref().map(|t| t.0.clone()) else {
        return HttpResponse::BadRequest().body("api_error_missing_conversation_id");
    };
    let project_id = form.project_id.as_ref().map(|t| t.0.clone());

    // The conversation may not be persisted yet (fresh chat): create it so the
    // file's conversation_id FK holds. A later message send updates it in place.
    let conv_repo = db.repository::<Conversation>();
    match conv_repo.read(&conversation_id).await {
        Ok(Some(c)) if c.owner != username => return HttpResponse::Forbidden().finish(),
        Ok(Some(_)) => {}
        Ok(None) => {
            let now = Utc::now().to_rfc3339();
            let conv = Conversation {
                id: conversation_id.clone(),
                // Blank until the first message is sent (see title logic in
                // stream_message), so the message text becomes the title.
                title: String::new(),
                active_message_id: None,
                owner: username.clone(),
                project_id: project_id.clone(),
                updated_at: now,
            };
            if let Err(e) = conv_repo.create(&conv).await {
                tracing::error!("Failed to create conversation for attachment: {}", e);
                return HttpResponse::InternalServerError()
                    .body("api_error_conversation_create_failed");
            }
        }
        Err(e) => {
            tracing::error!("Internal error: {}", e);
            return HttpResponse::InternalServerError().body("api_error_internal");
        }
    }

    // create_uploaded_file expects exactly one scope; attach to the conversation.
    form.project_id = None;

    match create_uploaded_file(&db, switchboard.get_ref(), vllm.get_ref(), &username, form).await {
        Ok(file) => HttpResponse::Ok()
            .content_type("text/html")
            .body(render_attachment_chip(&file, true).render()),
        Err(resp) => resp,
    }
}

/// Remove a staged (not-yet-sent) attachment. Only deletes files still owned by
/// the user and not yet linked to a message. Returns an empty body so the chip
/// is swapped out.
#[post("/detach/{file_id}")]
pub async fn detach(
    req: actix_web::HttpRequest,
    jwt_config: web::Data<JwtConfig>,
    db: web::Data<Db>,
    file_id: web::Path<String>,
) -> impl Responder {
    let username = match get_user_from_req(&req, &jwt_config).await {
        Some(claims) => claims.sub,
        None => return HttpResponse::Unauthorized().finish(),
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
    HttpResponse::Ok().content_type("text/html").body("")
}

/// Return the current chip for a staged file. Polled by the composer while the
/// file is still processing; an empty body (file gone) removes the chip.
#[get("/chip/{file_id}")]
pub async fn chip_status(
    req: actix_web::HttpRequest,
    jwt_config: web::Data<JwtConfig>,
    db: web::Data<Db>,
    file_id: web::Path<String>,
) -> impl Responder {
    let username = match get_user_from_req(&req, &jwt_config).await {
        Some(claims) => claims.sub,
        None => return HttpResponse::Unauthorized().finish(),
    };

    match db.repository::<File>().read(&file_id).await {
        Ok(Some(f)) if f.owner == username => HttpResponse::Ok()
            .content_type("text/html")
            .body(render_attachment_chip(&f, true).render()),
        Ok(Some(_)) => HttpResponse::Forbidden().finish(),
        // File no longer exists: empty body swaps the chip out.
        Ok(None) => HttpResponse::Ok().content_type("text/html").body(""),
        Err(e) => {
            tracing::error!("Internal error: {}", e);
            HttpResponse::InternalServerError().body("api_error_internal")
        }
    }
}

/// Retry processing a failed staged file. Returns a chip in the processing
/// state so the composer resumes polling for the new status.
#[post("/reprocess/{file_id}")]
pub async fn reprocess(
    req: actix_web::HttpRequest,
    jwt_config: web::Data<JwtConfig>,
    db: web::Data<Db>,
    switchboard: web::Data<SwitchboardClient>,
    vllm: web::Data<VllmClient>,
    file_id: web::Path<String>,
) -> impl Responder {
    let username = match get_user_from_req(&req, &jwt_config).await {
        Some(claims) => claims.sub,
        None => return HttpResponse::Unauthorized().finish(),
    };

    match db.repository::<File>().read(&file_id).await {
        Ok(Some(mut f)) if f.owner == username => {
            pipeline::spawn_processing(
                db.get_ref().clone(),
                switchboard.get_ref().clone(),
                vllm.get_ref().clone(),
                f.id.clone(),
            );
            // Reflect the imminent state so the returned chip polls again.
            f.status = STATUS_PROCESSING.to_string();
            f.error_message = None;
            HttpResponse::Ok()
                .content_type("text/html")
                .body(render_attachment_chip(&f, true).render())
        }
        Ok(Some(_)) => HttpResponse::Forbidden().finish(),
        Ok(None) => HttpResponse::NotFound().body("api_error_file_not_found"),
        Err(e) => {
            tracing::error!("Internal error: {}", e);
            HttpResponse::InternalServerError().body("api_error_internal")
        }
    }
}

/// Confirmation modal for deleting a project file from the sidebar. Mirrors the
/// conversation delete modal so both flows look and behave identically.
#[get("/delete-modal/{file_id}")]
pub async fn delete_modal(
    req: actix_web::HttpRequest,
    jwt_config: web::Data<JwtConfig>,
    db: web::Data<Db>,
    file_id: web::Path<String>,
) -> impl Responder {
    let username = match get_user_from_req(&req, &jwt_config).await {
        Some(claims) => claims.sub,
        None => return HttpResponse::Unauthorized().finish(),
    };

    let name = match db.repository::<File>().read(&file_id).await {
        Ok(Some(f)) if f.owner == username => format!("\"{}\"", f.file_name),
        Ok(Some(_)) => return HttpResponse::Forbidden().finish(),
        Ok(None) => return HttpResponse::NotFound().body("api_error_file_not_found"),
        Err(e) => {
            tracing::error!("Internal error: {}", e);
            return HttpResponse::InternalServerError().body("api_error_internal");
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

    HttpResponse::Ok()
        .content_type("text/html")
        .body(modal.render())
}

/// Delete a project file from the sidebar. Returns a closed modal plus an
/// out-of-band swap that removes the file's row. Blobs and chunks cascade.
#[post("/delete/{file_id}")]
pub async fn delete_file_ui(
    req: actix_web::HttpRequest,
    jwt_config: web::Data<JwtConfig>,
    db: web::Data<Db>,
    file_id: web::Path<String>,
) -> impl Responder {
    let username = match get_user_from_req(&req, &jwt_config).await {
        Some(claims) => claims.sub,
        None => return HttpResponse::Unauthorized().finish(),
    };

    let repo = db.repository::<File>();
    match repo.read(&file_id).await {
        Ok(Some(f)) if f.owner == username => {
            if let Err(e) = repo.delete(&f.id).await {
                tracing::error!("Failed to delete file {}: {}", f.id, e);
                return HttpResponse::InternalServerError().body("api_error_internal");
            }
        }
        Ok(Some(_)) => return HttpResponse::Forbidden().finish(),
        Ok(None) => return HttpResponse::NotFound().body("api_error_file_not_found"),
        Err(e) => {
            tracing::error!("Internal error: {}", e);
            return HttpResponse::InternalServerError().body("api_error_internal");
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

    HttpResponse::Ok()
        .content_type("text/html")
        .body(format!("{}{}", close_modal, oob_delete))
}

pub fn scope() -> actix_web::Scope {
    web::scope("/files")
        .service(attach)
        .service(detach)
        .service(chip_status)
        .service(reprocess)
        .service(delete_modal)
        .service(delete_file_ui)
}
