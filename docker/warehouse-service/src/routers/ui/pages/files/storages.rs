use crate::domain::storage::{self, DynamicStorage, NewStorage, StorageUpdate};
use crate::domain::storage_file;
use crate::routers::files::dynamic;
use crate::routers::ui::authz::{can_manage, require_manage, ui_claims};
use crate::routers::ui::common::{
    UiPageKind, is_ui_authenticated, render_page, ui_login_redirect, ui_path,
};
use actix_web::http::header::ContentType;
use actix_web::{HttpRequest, HttpResponse, Responder, get, post, web};
use quench_auth::prelude::JwtConfig;
use quench_db::prelude::Db;
use quench_starter::prelude::with_base_path;
use quench_web::prelude::*;
use quench_web_components::containers::empty_state;

const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
const MIB: f64 = 1024.0 * 1024.0;

/// The first-page cap for the in-browser file listing. Browsing is a
/// convenience here, not the backup client's paged `GET /api/v1/files/{s}`;
/// a storage with more than this many files shows the first slice and says so.
const FILE_BROWSE_LIMIT: i64 = 200;

// ---------------------------------------------------------------------------
// Query / form shapes
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize)]
pub struct FilesQuery {
    pub storage: Option<String>,
}

#[derive(serde::Deserialize)]
pub struct CreateStorageForm {
    pub name: String,
    pub owner: String,
    #[serde(default)]
    pub quota_gib: String,
    #[serde(default)]
    pub max_file_mib: String,
    #[serde(default)]
    pub sync_enabled: Option<String>,
}

#[derive(serde::Deserialize)]
pub struct EditStorageForm {
    #[serde(default)]
    pub quota_gib: String,
    #[serde(default)]
    pub max_file_mib: String,
    #[serde(default)]
    pub clear_max_file: Option<String>,
    #[serde(default)]
    pub sync_enabled: Option<String>,
}

#[derive(serde::Deserialize)]
pub struct DeleteStorageForm {
    pub name: String,
}

#[derive(serde::Deserialize)]
pub struct DeleteFileForm {
    pub storage: String,
    pub path: String,
}

#[derive(serde::Deserialize)]
pub struct DeleteStorageModalQuery {
    pub storage: String,
}

// ---------------------------------------------------------------------------
// View model (what `render_storages_page` consumes - kept free of `Db` so it
// is a pure function the tests can drive directly)
// ---------------------------------------------------------------------------

pub struct FileRow {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: Option<i64>,
}

pub struct SelectedView {
    pub name: String,
    /// `Some` for a database-backed storage, `None` for a static one.
    pub dynamic: Option<DynamicStorage>,
    pub static_root: Option<String>,
    pub files: Vec<FileRow>,
    pub truncated: bool,
    /// An i18n key describing why the file listing is empty, when that is a
    /// condition rather than a genuinely empty storage.
    pub notice: Option<&'static str>,
}

#[derive(Default)]
pub struct StoragesView {
    pub static_names: Vec<String>,
    pub dynamic: Vec<DynamicStorage>,
    pub selected: Option<SelectedView>,
}

// ---------------------------------------------------------------------------
// GET /ui/files/storages
// ---------------------------------------------------------------------------

#[get("/files/storages")]
pub async fn files_storages(
    req: HttpRequest,
    query: web::Query<FilesQuery>,
    config: web::Data<JwtConfig>,
    db: web::Data<Db>,
) -> impl Responder {
    handle_list(&req, &query, &config, &db).await
}

#[get("/files/storages/")]
pub async fn files_storages_slash(
    req: HttpRequest,
    query: web::Query<FilesQuery>,
    config: web::Data<JwtConfig>,
    db: web::Data<Db>,
) -> impl Responder {
    handle_list(&req, &query, &config, &db).await
}

async fn handle_list(
    req: &HttpRequest,
    query: &FilesQuery,
    config: &JwtConfig,
    db: &Db,
) -> HttpResponse {
    if !is_ui_authenticated(req, config).await {
        return ui_login_redirect();
    }

    let manage = ui_claims(req, config)
        .await
        .is_some_and(|claims| can_manage(&claims));

    let static_names: Vec<String> = crate::routers::files::storages()
        .iter()
        .map(|storage| storage.name.clone())
        .collect();

    let dynamic = if crate::routers::files_enabled() {
        storage::list(db).await.unwrap_or_default()
    } else {
        Vec::new()
    };

    let selected = match query.storage.as_deref() {
        Some(name) if !name.is_empty() => Some(build_selection(db, name, &dynamic).await),
        _ => None,
    };

    render_storages_page(
        &StoragesView {
            static_names,
            dynamic,
            selected,
        },
        manage,
    )
}

async fn build_selection(db: &Db, name: &str, dynamic: &[DynamicStorage]) -> SelectedView {
    if let Some(found) = dynamic.iter().find(|s| s.name == name) {
        let (files, truncated) = dynamic_files(db, name).await;
        return SelectedView {
            name: name.to_string(),
            dynamic: Some(found.clone()),
            static_root: None,
            files,
            truncated,
            notice: None,
        };
    }

    if let Some(storage) = crate::routers::files::storage(name) {
        let (files, notice) = static_files(&storage.root).await;
        return SelectedView {
            name: name.to_string(),
            dynamic: None,
            static_root: Some(storage.root.display().to_string()),
            files,
            truncated: false,
            notice,
        };
    }

    SelectedView {
        name: name.to_string(),
        dynamic: None,
        static_root: None,
        files: Vec::new(),
        truncated: false,
        notice: Some("ui_storage_not_found"),
    }
}

async fn dynamic_files(db: &Db, name: &str) -> (Vec<FileRow>, bool) {
    let rows = storage_file::list_files_page(db, name, "", None, FILE_BROWSE_LIMIT + 1, false)
        .await
        .unwrap_or_default();
    let truncated = rows.len() as i64 > FILE_BROWSE_LIMIT;
    let files = rows
        .into_iter()
        .take(FILE_BROWSE_LIMIT as usize)
        .map(|file| {
            let name = file
                .path
                .rsplit('/')
                .next()
                .unwrap_or(&file.path)
                .to_string();
            FileRow {
                name,
                path: file.path,
                is_dir: false,
                size: Some(file.size),
            }
        })
        .collect();
    (files, truncated)
}

/// A shallow read of a static storage's root - enough to see what is there
/// without walking an arbitrarily deep tree in a page render.
async fn static_files(root: &std::path::Path) -> (Vec<FileRow>, Option<&'static str>) {
    let Ok(mut reader) = tokio::fs::read_dir(root).await else {
        return (Vec::new(), Some("ui_storage_root_unreadable"));
    };

    let mut files = Vec::new();
    while let Ok(Some(entry)) = reader.next_entry().await {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') && name.ends_with(".part") {
            continue;
        }
        let Ok(metadata) = entry.metadata().await else {
            continue;
        };
        files.push(FileRow {
            name: name.clone(),
            path: name,
            is_dir: metadata.is_dir(),
            size: metadata.is_file().then_some(metadata.len() as i64),
        });
    }
    files.sort_by(|a, b| {
        (a.is_dir.cmp(&b.is_dir))
            .reverse()
            .then_with(|| a.name.cmp(&b.name))
    });
    (files, None)
}

// ---------------------------------------------------------------------------
// POST /ui/files/storages  (create)
// ---------------------------------------------------------------------------

#[post("/files/storages")]
pub async fn create_storage(
    req: HttpRequest,
    form: web::Form<CreateStorageForm>,
    config: web::Data<JwtConfig>,
    db: web::Data<Db>,
) -> impl Responder {
    if let Err(response) = require_manage(&req, &config).await {
        return response;
    }
    if !crate::routers::files_enabled() {
        return HttpResponse::NotFound().body("api_error_files_disabled");
    }

    let name = form.name.trim().to_string();
    let owner = form.owner.trim().to_string();

    if !crate::routers::files::valid_storage_name(&name) {
        return HttpResponse::BadRequest().body("api_error_invalid_storage_name");
    }
    if owner.is_empty() {
        return HttpResponse::BadRequest().body("api_error_storage_owner_required");
    }
    if crate::routers::files::storage(&name).is_some() {
        return HttpResponse::Conflict().body("api_error_storage_name_static_clash");
    }

    let quota_bytes = match parse_scaled(&form.quota_gib, GIB) {
        Ok(Some(bytes)) => bytes,
        Ok(None) => dynamic::default_quota_bytes(),
        Err(()) => return HttpResponse::BadRequest().body("api_error_invalid_quota"),
    };
    let max_file_bytes = match parse_scaled(&form.max_file_mib, MIB) {
        Ok(value) => value,
        Err(()) => return HttpResponse::BadRequest().body("api_error_invalid_max_file"),
    };

    let new = NewStorage {
        name: name.clone(),
        owner,
        max_file_bytes,
        quota_bytes,
        sync_enabled: checkbox_on(&form.sync_enabled),
    };

    match storage::create(&db, &new).await {
        Ok(created) => redirect_to_storage(&created.name),
        Err(problem) if problem.is_unique_violation() => {
            HttpResponse::Conflict().body("api_error_storage_exists")
        }
        Err(problem) if problem.is_foreign_key_violation() => {
            HttpResponse::BadRequest().body("api_error_storage_owner_unknown")
        }
        Err(problem) => {
            tracing::error!("UI create dynamic storage failed: {problem}");
            HttpResponse::InternalServerError().body("api_error_internal")
        }
    }
}

// ---------------------------------------------------------------------------
// POST /ui/files/storages/{name}/edit
// ---------------------------------------------------------------------------

#[post("/files/storages/{name}/edit")]
pub async fn edit_storage(
    req: HttpRequest,
    path: web::Path<String>,
    form: web::Form<EditStorageForm>,
    config: web::Data<JwtConfig>,
    db: web::Data<Db>,
) -> impl Responder {
    if let Err(response) = require_manage(&req, &config).await {
        return response;
    }
    if !crate::routers::files_enabled() {
        return HttpResponse::NotFound().body("api_error_files_disabled");
    }

    let name = path.into_inner();

    let quota_bytes = match parse_scaled(&form.quota_gib, GIB) {
        Ok(value) => value,
        Err(()) => return HttpResponse::BadRequest().body("api_error_invalid_quota"),
    };

    let max_file_bytes = if checkbox_on(&form.clear_max_file) {
        Some(None)
    } else {
        match parse_scaled(&form.max_file_mib, MIB) {
            Ok(Some(bytes)) => Some(Some(bytes)),
            Ok(None) => None,
            Err(()) => return HttpResponse::BadRequest().body("api_error_invalid_max_file"),
        }
    };

    let changes = StorageUpdate {
        max_file_bytes,
        quota_bytes,
        sync_enabled: Some(checkbox_on(&form.sync_enabled)),
    };

    match storage::update(&db, &name, &changes).await {
        Ok(Some(updated)) => redirect_to_storage(&updated.name),
        Ok(None) => HttpResponse::NotFound().body("api_error_storage_not_found"),
        Err(problem) => {
            tracing::error!("UI edit dynamic storage failed: {problem}");
            HttpResponse::InternalServerError().body("api_error_internal")
        }
    }
}

// ---------------------------------------------------------------------------
// POST /ui/files/delete-storage  (+ its confirm modal)
// ---------------------------------------------------------------------------

#[get("/files/delete-storage-modal")]
pub async fn delete_storage_modal(
    req: HttpRequest,
    query: web::Query<DeleteStorageModalQuery>,
    config: web::Data<JwtConfig>,
) -> impl Responder {
    if !is_ui_authenticated(&req, &config).await {
        return ui_login_redirect();
    }
    HttpResponse::Ok()
        .content_type(ContentType::html())
        .body(render_delete_storage_modal(&query.storage))
}

#[get("/files/delete-storage-modal/empty")]
pub async fn empty_delete_storage_modal(
    req: HttpRequest,
    config: web::Data<JwtConfig>,
) -> impl Responder {
    if !is_ui_authenticated(&req, &config).await {
        return ui_login_redirect();
    }
    HttpResponse::Ok()
        .content_type(ContentType::html())
        .body(empty_delete_storage_modal_element().render())
}

#[post("/files/delete-storage")]
pub async fn delete_storage(
    req: HttpRequest,
    form: web::Form<DeleteStorageForm>,
    config: web::Data<JwtConfig>,
    db: web::Data<Db>,
) -> impl Responder {
    if let Err(response) = require_manage(&req, &config).await {
        return response;
    }
    if !crate::routers::files_enabled() {
        return HttpResponse::NotFound().body("api_error_files_disabled");
    }

    let name = form.name.clone();

    let Ok(Some(found)) = storage::read(&db, &name).await else {
        return HttpResponse::NotFound().body("api_error_storage_not_found");
    };

    // Mirror `routers::files::ops::storages::remove`: release each file (and
    // its blob ref-count) before dropping the storage row.
    if let Some(root) = dynamic::root()
        && let Ok(files) = storage_file::list_files(&db, &found.name, "").await
    {
        for file in files {
            let _ = storage_file::delete_file(&db, &found.name, &file.path, |sha256| {
                dynamic::blob_path(&root, sha256)
            })
            .await;
        }
    }

    match storage::delete(&db, &found.name).await {
        Ok(_) => HttpResponse::NoContent()
            .append_header(("HX-Redirect", with_base_path("/ui/files/storages")))
            .finish(),
        Err(problem) => {
            tracing::error!("UI delete dynamic storage failed: {problem}");
            HttpResponse::InternalServerError().body("api_error_internal")
        }
    }
}

// ---------------------------------------------------------------------------
// POST /ui/files/delete-file
// ---------------------------------------------------------------------------

#[post("/files/delete-file")]
pub async fn delete_file(
    req: HttpRequest,
    form: web::Form<DeleteFileForm>,
    config: web::Data<JwtConfig>,
    db: web::Data<Db>,
) -> impl Responder {
    if let Err(response) = require_manage(&req, &config).await {
        return response;
    }
    if !crate::routers::files_enabled() {
        return HttpResponse::NotFound().body("api_error_files_disabled");
    }

    let storage_name = form.storage.clone();
    let path = form.path.clone();

    if crate::routers::files::relative(&path).is_err() {
        return HttpResponse::BadRequest().body("api_error_invalid_path");
    }

    // Dynamic storage: the domain layer owns the blob ref-count and quota.
    if storage::read(&db, &storage_name)
        .await
        .ok()
        .flatten()
        .is_some()
    {
        let Some(root) = dynamic::root() else {
            return HttpResponse::InternalServerError().body("api_error_no_dynamic_root");
        };
        return match storage_file::delete_file(&db, &storage_name, &path, |sha256| {
            dynamic::blob_path(&root, sha256)
        })
        .await
        {
            Ok(true) => redirect_to_storage(&storage_name),
            Ok(false) => HttpResponse::NotFound().body("api_error_file_not_found"),
            Err(problem) => {
                tracing::error!("UI delete dynamic file failed: {problem}");
                HttpResponse::InternalServerError().body("api_error_internal")
            }
        };
    }

    // Static storage: a plain unlink, confined to the storage root.
    let Some(storage) = crate::routers::files::storage(&storage_name) else {
        return HttpResponse::NotFound().body("api_error_storage_not_found");
    };
    let Ok(target) = crate::routers::files::resolve(storage, &path) else {
        return HttpResponse::BadRequest().body("api_error_invalid_path");
    };
    if !crate::routers::files::confined(&storage.root, &target).await {
        return HttpResponse::Forbidden().body("api_error_path_escapes_storage");
    }
    match tokio::fs::remove_file(&target).await {
        Ok(()) => redirect_to_storage(&storage_name),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            HttpResponse::NotFound().body("api_error_file_not_found")
        }
        Err(err) => {
            tracing::error!("UI delete static file failed: {err}");
            HttpResponse::InternalServerError().body("api_error_internal")
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// A checkbox is only submitted when checked, so any value present means on.
fn checkbox_on(value: &Option<String>) -> bool {
    value.as_deref().is_some_and(|v| !v.is_empty())
}

/// Parse an optional user-entered amount in units of `scale` bytes. Blank ->
/// `Ok(None)` ("leave / use default"); a non-negative number -> `Ok(Some(bytes))`;
/// anything else -> `Err(())`.
fn parse_scaled(raw: &str, scale: f64) -> Result<Option<i64>, ()> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let value: f64 = trimmed.parse().map_err(|_| ())?;
    if !value.is_finite() || value < 0.0 {
        return Err(());
    }
    Ok(Some((value * scale).round() as i64))
}

fn format_bytes(bytes: i64) -> String {
    let b = bytes as f64;
    if b >= GIB {
        format!("{:.2} GiB", b / GIB)
    } else if b >= MIB {
        format!("{:.2} MiB", b / MIB)
    } else if b >= 1024.0 {
        format!("{:.2} KiB", b / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

fn gib_string(bytes: i64) -> String {
    format!("{:.2}", bytes as f64 / GIB)
}

fn mib_string(bytes: i64) -> String {
    format!("{:.2}", bytes as f64 / MIB)
}

fn redirect_to_storage(name: &str) -> HttpResponse {
    HttpResponse::NoContent()
        .append_header((
            "HX-Redirect",
            with_base_path(&format!("/ui/files/storages?storage={name}")),
        ))
        .finish()
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

pub fn render_storages_page(view: &StoragesView, can_manage: bool) -> HttpResponse {
    let left = div()
        .class("split-left panel")
        .child(
            div()
                .class("panel-title")
                .attr("data-i18n", "ui_storages_title"),
        )
        .child(div().class("tree-scroll").child(render_storage_list(view)));

    let right = div()
        .class("split-right panel")
        .child(render_detail_panel(view, can_manage));

    render_page(
        HttpResponse::Ok(),
        content()
            .class("container-fluid py-4")
            .child(div().class("split-view").child(left).child(right))
            .child(empty_delete_storage_modal_element()),
        UiPageKind::Files,
    )
}

fn render_storage_list(view: &StoragesView) -> Element {
    if view.static_names.is_empty() && view.dynamic.is_empty() {
        return empty_state("ui_storages_empty");
    }

    let selected = view.selected.as_ref().map(|s| s.name.as_str());
    let mut list = ul().class("repo-tree");

    for name in &view.static_names {
        list = list.child(storage_list_item(
            name,
            "fas fa-hard-drive mr-2",
            true,
            selected,
        ));
    }
    for storage in &view.dynamic {
        list = list.child(
            li().child(
                a().attr(
                    "href",
                    format!("{}?storage={}", ui_path("/files/storages"), storage.name),
                )
                .class(if selected == Some(storage.name.as_str()) {
                    "repo-link active"
                } else {
                    "repo-link"
                })
                .child(i().class("fas fa-box-archive mr-2"))
                .child(span().text(&storage.name))
                .child(
                    span()
                        .class("storage-owner")
                        .text(format!(" · {}", storage.owner)),
                ),
            ),
        );
    }
    list
}

fn storage_list_item(name: &str, icon: &str, _is_static: bool, selected: Option<&str>) -> Element {
    li().child(
        a().attr(
            "href",
            format!("{}?storage={name}", ui_path("/files/storages")),
        )
        .class(if selected == Some(name) {
            "repo-link active"
        } else {
            "repo-link"
        })
        .child(i().class(icon))
        .child(span().text(name))
        .child(
            span()
                .class("storage-badge")
                .attr("data-i18n", "ui_storage_static_badge"),
        ),
    )
}

fn render_detail_panel(view: &StoragesView, can_manage: bool) -> Element {
    let Some(selected) = view.selected.as_ref() else {
        let mut body = div()
            .class("manage-scroll")
            .child(empty_state("ui_storages_select"));
        if can_manage {
            body = body.child(render_create_form());
        }
        return div()
            .class("h-100 d-flex flex-column")
            .child(
                div()
                    .class("panel-title")
                    .attr("data-i18n", "ui_storages_detail_title"),
            )
            .child(body);
    };

    let title = div()
        .class("panel-title")
        .child(span().attr("data-i18n", "ui_metadata_for"))
        .child(span().text(format!(" {}", selected.name)));

    let mut body = div().class("manage-scroll");

    match &selected.dynamic {
        Some(storage) => {
            body = body.child(render_dynamic_meta(storage));
            if can_manage {
                body = body.child(render_edit_form(storage));
                body = body.child(render_delete_button(&storage.name));
            }
        }
        None => {
            body = body.child(render_static_meta(selected));
        }
    }

    body = body.child(render_file_list(selected, can_manage));

    if can_manage {
        body = body.child(render_create_form());
    }

    div()
        .class("h-100 d-flex flex-column")
        .child(title)
        .child(body)
}

fn render_dynamic_meta(storage: &DynamicStorage) -> Element {
    let pct = if storage.quota_bytes > 0 {
        ((storage.used_bytes as f64 / storage.quota_bytes as f64) * 100.0).clamp(0.0, 100.0)
    } else {
        0.0
    };

    let max_file = match storage.max_file_bytes {
        Some(bytes) => format_bytes(bytes),
        None => "—".to_string(),
    };

    div()
        .class("meta-list")
        .child(meta_row("ui_storage_kind", "dynamic"))
        .child(meta_row("ui_storage_owner", &storage.owner))
        .child(meta_row_value(
            "ui_storage_usage",
            div()
                .class("quota-bar")
                .child(
                    div()
                        .class("quota-bar-fill")
                        .attr("style", format!("width: {pct:.1}%;")),
                )
                .child(span().class("quota-bar-label").text(format!(
                    "{} / {} ({pct:.1}%)",
                    format_bytes(storage.used_bytes),
                    format_bytes(storage.quota_bytes)
                ))),
        ))
        .child(meta_row("ui_storage_max_file", &max_file))
        .child(meta_row_value(
            "ui_storage_sync",
            span().attr(
                "data-i18n",
                if storage.sync_enabled {
                    "ui_storage_sync_on"
                } else {
                    "ui_storage_sync_off"
                },
            ),
        ))
        .child(meta_row(
            "ui_storage_created",
            &storage.created_at.format("%Y-%m-%d %H:%M UTC").to_string(),
        ))
}

fn render_static_meta(selected: &SelectedView) -> Element {
    div()
        .class("meta-list")
        .child(meta_row("ui_storage_kind", "static"))
        .child(meta_row(
            "ui_storage_root",
            selected.static_root.as_deref().unwrap_or("—"),
        ))
}

fn render_file_list(selected: &SelectedView, can_manage: bool) -> Element {
    let mut section = div().class("mt-4").child(
        div()
            .class("panel-subtitle")
            .attr("data-i18n", "ui_storage_files_title"),
    );

    if let Some(notice) = selected.notice {
        return section.child(empty_state(notice));
    }
    if selected.files.is_empty() {
        return section.child(empty_state("ui_storage_files_empty"));
    }

    let mut list = ul().class("file-list");
    for file in &selected.files {
        let icon = if file.is_dir {
            "fas fa-folder mr-2"
        } else {
            "fas fa-file mr-2"
        };
        let size = match file.size {
            Some(bytes) => format_bytes(bytes),
            None => String::new(),
        };

        let mut row = li()
            .class("file-row")
            .child(i().class(icon))
            .child(span().class("file-name").text(&file.name))
            .child(span().class("file-size mono").text(size));

        if !file.is_dir {
            row = row.child(
                a().class("file-download")
                    .attr(
                        "href",
                        with_base_path(&format!(
                            "/api/v1/files/{}/download?path={}",
                            selected.name,
                            encode_query_component(&file.path)
                        )),
                    )
                    .attr("data-i18n", "ui_file_download")
                    .text("download"),
            );
        }

        if can_manage && !file.is_dir {
            row = row.child(
                form()
                    .class("inline-action-form")
                    .attr("hx-post", ui_path("/files/delete-file"))
                    .attr("hx-swap", "none")
                    .child(
                        input()
                            .attr("type", "hidden")
                            .attr("name", "storage")
                            .attr("value", &selected.name),
                    )
                    .child(
                        input()
                            .attr("type", "hidden")
                            .attr("name", "path")
                            .attr("value", &file.path),
                    )
                    .child(
                        button()
                            .class("button-danger-sm")
                            .attr("type", "submit")
                            .attr("data-i18n", "ui_file_delete")
                            .text("Delete"),
                    ),
            );
        }

        list = list.child(row);
    }

    section = section.child(list);
    if selected.truncated {
        section = section.child(
            div()
                .class("file-truncated")
                .attr("data-i18n", "ui_storage_files_truncated"),
        );
    }
    section
}

fn render_edit_form(storage: &DynamicStorage) -> Element {
    let sync_checkbox = {
        let mut cb = checkbox().attr("name", "sync_enabled").attr("value", "on");
        if storage.sync_enabled {
            cb = cb.attr("checked", "checked");
        }
        cb
    };

    form()
        .class("storage-form mt-4")
        .attr(
            "hx-post",
            ui_path(&format!("/files/storages/{}/edit", storage.name)),
        )
        .attr("hx-swap", "none")
        .child(
            div()
                .class("panel-subtitle")
                .attr("data-i18n", "ui_storage_edit_title"),
        )
        .child(field_row(
            "ui_storage_quota_gib",
            number_input("quota_gib", &gib_string(storage.quota_bytes)),
        ))
        .child(field_row(
            "ui_storage_max_file_mib",
            number_input(
                "max_file_mib",
                &storage.max_file_bytes.map(mib_string).unwrap_or_default(),
            ),
        ))
        .child(field_row(
            "ui_storage_clear_max_file",
            checkbox()
                .attr("name", "clear_max_file")
                .attr("value", "on"),
        ))
        .child(field_row("ui_storage_sync", sync_checkbox))
        .child(
            button()
                .class("button")
                .attr("type", "submit")
                .attr("data-i18n", "ui_storage_save"),
        )
}

fn render_create_form() -> Element {
    form()
        .class("storage-form mt-4")
        .attr("hx-post", ui_path("/files/storages"))
        .attr("hx-swap", "none")
        .child(
            div()
                .class("panel-subtitle")
                .attr("data-i18n", "ui_storage_new_title"),
        )
        .child(field_row("ui_storage_name", text_input("name")))
        .child(field_row("ui_storage_owner", text_input("owner")))
        .child(field_row(
            "ui_storage_quota_gib",
            number_input("quota_gib", ""),
        ))
        .child(field_row(
            "ui_storage_max_file_mib",
            number_input("max_file_mib", ""),
        ))
        .child(field_row(
            "ui_storage_sync",
            checkbox().attr("name", "sync_enabled").attr("value", "on"),
        ))
        .child(
            button()
                .class("button")
                .attr("type", "submit")
                .attr("data-i18n", "ui_storage_create"),
        )
}

fn render_delete_button(name: &str) -> Element {
    div().class("mt-4").child(
        button()
            .class("button-danger-sm")
            .attr("type", "button")
            .attr(
                "hx-get",
                format!(
                    "{}?storage={}",
                    ui_path("/files/delete-storage-modal"),
                    encode_query_component(name)
                ),
            )
            .attr("hx-target", "#confirm-delete-storage-modal")
            .attr("hx-swap", "outerHTML")
            .child(i().class("fas fa-trash mr-2"))
            .child(
                span()
                    .attr("data-i18n", "ui_storage_delete")
                    .text("Delete storage"),
            ),
    )
}

pub fn render_delete_storage_modal(name: &str) -> String {
    div()
        .attr("id", "confirm-delete-storage-modal")
        .class("open")
        .child(
            button()
                .class("confirm-modal-backdrop")
                .attr("type", "button")
                .attr("hx-get", ui_path("/files/delete-storage-modal/empty"))
                .attr("hx-target", "#confirm-delete-storage-modal")
                .attr("hx-swap", "outerHTML"),
        )
        .child(
            div()
                .class("confirm-modal-content")
                .child(
                    div()
                        .class("confirm-modal-header")
                        .child(
                            div()
                                .class("confirm-modal-title")
                                .attr("data-i18n", "ui_storage_delete_title")
                                .text("Delete storage"),
                        )
                        .child(
                            button()
                                .class("confirm-modal-close")
                                .attr("type", "button")
                                .attr("hx-get", ui_path("/files/delete-storage-modal/empty"))
                                .attr("hx-target", "#confirm-delete-storage-modal")
                                .attr("hx-swap", "outerHTML")
                                .child(i().class("fa-solid fa-xmark")),
                        ),
                )
                .child(
                    div()
                        .class("confirm-modal-body")
                        .child(
                            p().attr("data-i18n", "ui_storage_delete_confirm_text").text(
                                "Delete this storage and everything in it? This cannot be undone.",
                            ),
                        )
                        .child(div().class("confirm-delete-target").text(name))
                        .child(
                            form()
                                .class("confirm-actions")
                                .attr("hx-post", ui_path("/files/delete-storage"))
                                .attr("hx-swap", "none")
                                .child(
                                    input()
                                        .attr("type", "hidden")
                                        .attr("name", "name")
                                        .attr("value", name),
                                )
                                .child(
                                    button()
                                        .class("button cancel")
                                        .attr("type", "button")
                                        .attr(
                                            "hx-get",
                                            ui_path("/files/delete-storage-modal/empty"),
                                        )
                                        .attr("hx-target", "#confirm-delete-storage-modal")
                                        .attr("hx-swap", "outerHTML")
                                        .attr("data-i18n", "ui_common_cancel")
                                        .text("Cancel"),
                                )
                                .child(
                                    button()
                                        .class("button delete")
                                        .attr("type", "submit")
                                        .attr("data-i18n", "ui_common_delete")
                                        .text("Delete"),
                                ),
                        ),
                ),
        )
        .render()
}

fn empty_delete_storage_modal_element() -> Element {
    div().attr("id", "confirm-delete-storage-modal")
}

// ---------------------------------------------------------------------------
// Small element helpers
// ---------------------------------------------------------------------------

fn field_row(label_key: &str, control: Element) -> Element {
    div()
        .class("field-row")
        .child(label().class("field-label").attr("data-i18n", label_key))
        .child(control)
}

fn text_input(name: &str) -> Element {
    input().attr("type", "text").attr("name", name)
}

fn number_input(name: &str, value: &str) -> Element {
    input()
        .attr("type", "number")
        .attr("name", name)
        .attr("min", "0")
        .attr("step", "0.01")
        .attr("value", value)
}

fn meta_row(label_key: &str, value: &str) -> Element {
    meta_row_value(label_key, span().text(value))
}

fn meta_row_value(label_key: &str, value: Element) -> Element {
    div()
        .class("meta-row")
        .child(div().class("meta-label").attr("data-i18n", label_key))
        .child(div().class("meta-value mono").child(value))
}

fn encode_query_component(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}
