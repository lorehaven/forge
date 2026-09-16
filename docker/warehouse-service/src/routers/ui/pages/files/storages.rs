use crate::domain::storage::{self, DynamicStorage, NewStorage, StorageUpdate};
use crate::domain::storage_file;
use crate::routers::files::dynamic;
use crate::routers::ui::authz::{ManageGate, OptionalUiClaims, can_manage};
use crate::routers::ui::common::{PageAuth, UiPageKind, render_page, ui_login_redirect, ui_path};
use quench_db::prelude::Db;
use quench_http::prelude::{Form, Inject, Path, Query, Response, get, http::StatusCode, post};
use quench_starter::common::routes::with_base_path;
use quench_web::prelude::*;
use quench_web_components::containers::empty_state;

const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
const MIB: f64 = 1024.0 * 1024.0;

// --- Query / form shapes ---

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

// --- View model (kept free of `Db` so tests can drive `render_storages_page` directly) ---

pub struct SelectedView {
    pub name: String,
    /// `Some` for a database-backed storage, `None` for a static one.
    pub dynamic: Option<DynamicStorage>,
    pub static_root: Option<String>,
    /// An i18n key shown in place of the detail panel - set only when the
    /// selected name resolves to no storage at all.
    pub notice: Option<&'static str>,
}

#[derive(Default)]
pub struct StoragesView {
    pub static_names: Vec<String>,
    pub dynamic: Vec<DynamicStorage>,
    pub selected: Option<SelectedView>,
}

// --- GET /ui/files/storages ---

#[get("/ui/files/storages")]
pub async fn files_storages(
    auth: PageAuth,
    claims: OptionalUiClaims,
    Query(query): Query<FilesQuery>,
    Inject(db): Inject<Db>,
) -> Response {
    handle_list(auth, claims, &query, &db).await
}

#[get("/ui/files/storages/")]
pub async fn files_storages_slash(
    auth: PageAuth,
    claims: OptionalUiClaims,
    Query(query): Query<FilesQuery>,
    Inject(db): Inject<Db>,
) -> Response {
    handle_list(auth, claims, &query, &db).await
}

async fn handle_list(
    PageAuth(authenticated): PageAuth,
    OptionalUiClaims(claims): OptionalUiClaims,
    query: &FilesQuery,
    db: &Db,
) -> Response {
    if !authenticated {
        return ui_login_redirect();
    }

    let manage = claims.is_some_and(|claims| can_manage(&claims));

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
        Some(name) if !name.is_empty() => Some(build_selection(name, &dynamic)),
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

fn build_selection(name: &str, dynamic: &[DynamicStorage]) -> SelectedView {
    if let Some(found) = dynamic.iter().find(|s| s.name == name) {
        return SelectedView {
            name: name.to_string(),
            dynamic: Some(found.clone()),
            static_root: None,
            notice: None,
        };
    }

    if let Some(storage) = crate::routers::files::storage(name) {
        return SelectedView {
            name: name.to_string(),
            dynamic: None,
            static_root: Some(storage.root.display().to_string()),
            notice: None,
        };
    }

    SelectedView {
        name: name.to_string(),
        dynamic: None,
        static_root: None,
        notice: Some("ui_storage_not_found"),
    }
}

// --- POST /ui/files/storages (create) ---

#[post("/ui/files/storages")]
pub async fn create_storage(
    gate: ManageGate,
    Form(form): Form<CreateStorageForm>,
    Inject(db): Inject<Db>,
) -> Response {
    if let Err(response) = gate.or_response() {
        return response;
    }
    if !crate::routers::files_enabled() {
        return Response::text(StatusCode::NOT_FOUND, "api_error_files_disabled");
    }

    let name = form.name.trim().to_string();
    let owner = form.owner.trim().to_string();

    if !crate::routers::files::valid_storage_name(&name) {
        return Response::text(StatusCode::BAD_REQUEST, "api_error_invalid_storage_name");
    }
    if owner.is_empty() {
        return Response::text(StatusCode::BAD_REQUEST, "api_error_storage_owner_required");
    }
    if crate::routers::files::storage(&name).is_some() {
        return Response::text(StatusCode::CONFLICT, "api_error_storage_name_static_clash");
    }

    let quota_bytes = match parse_scaled(&form.quota_gib, GIB) {
        Ok(Some(bytes)) => bytes,
        Ok(None) => dynamic::default_quota_bytes(),
        Err(()) => return Response::text(StatusCode::BAD_REQUEST, "api_error_invalid_quota"),
    };
    let max_file_bytes = match parse_scaled(&form.max_file_mib, MIB) {
        Ok(value) => value,
        Err(()) => return Response::text(StatusCode::BAD_REQUEST, "api_error_invalid_max_file"),
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
            Response::text(StatusCode::CONFLICT, "api_error_storage_exists")
        }
        Err(problem) if problem.is_foreign_key_violation() => {
            Response::text(StatusCode::BAD_REQUEST, "api_error_storage_owner_unknown")
        }
        Err(problem) => {
            tracing::error!("UI create dynamic storage failed: {problem}");
            Response::text(StatusCode::INTERNAL_SERVER_ERROR, "api_error_internal")
        }
    }
}

// --- POST /ui/files/storages/{name}/edit ---

#[post("/ui/files/storages/{name}/edit")]
pub async fn edit_storage(
    gate: ManageGate,
    Path(name): Path<String>,
    Form(form): Form<EditStorageForm>,
    Inject(db): Inject<Db>,
) -> Response {
    if let Err(response) = gate.or_response() {
        return response;
    }
    if !crate::routers::files_enabled() {
        return Response::text(StatusCode::NOT_FOUND, "api_error_files_disabled");
    }

    let quota_bytes = match parse_scaled(&form.quota_gib, GIB) {
        Ok(value) => value,
        Err(()) => return Response::text(StatusCode::BAD_REQUEST, "api_error_invalid_quota"),
    };

    let max_file_bytes = if checkbox_on(&form.clear_max_file) {
        Some(None)
    } else {
        match parse_scaled(&form.max_file_mib, MIB) {
            Ok(Some(bytes)) => Some(Some(bytes)),
            Ok(None) => None,
            Err(()) => {
                return Response::text(StatusCode::BAD_REQUEST, "api_error_invalid_max_file");
            }
        }
    };

    let changes = StorageUpdate {
        max_file_bytes,
        quota_bytes,
        sync_enabled: Some(checkbox_on(&form.sync_enabled)),
    };

    match storage::update(&db, &name, &changes).await {
        Ok(Some(updated)) => redirect_to_storage(&updated.name),
        Ok(None) => Response::text(StatusCode::NOT_FOUND, "api_error_storage_not_found"),
        Err(problem) => {
            tracing::error!("UI edit dynamic storage failed: {problem}");
            Response::text(StatusCode::INTERNAL_SERVER_ERROR, "api_error_internal")
        }
    }
}

// --- POST /ui/files/delete-storage (+ its confirm modal) ---

#[get("/ui/files/delete-storage-modal")]
pub async fn delete_storage_modal(
    PageAuth(authenticated): PageAuth,
    Query(query): Query<DeleteStorageModalQuery>,
) -> Response {
    if !authenticated {
        return ui_login_redirect();
    }
    Response::html(StatusCode::OK, render_delete_storage_modal(&query.storage))
}

#[get("/ui/files/delete-storage-modal/empty")]
pub async fn empty_delete_storage_modal(PageAuth(authenticated): PageAuth) -> Response {
    if !authenticated {
        return ui_login_redirect();
    }
    Response::html(
        StatusCode::OK,
        empty_delete_storage_modal_element().render(),
    )
}

#[post("/ui/files/delete-storage")]
pub async fn delete_storage(
    gate: ManageGate,
    Form(form): Form<DeleteStorageForm>,
    Inject(db): Inject<Db>,
) -> Response {
    if let Err(response) = gate.or_response() {
        return response;
    }
    if !crate::routers::files_enabled() {
        return Response::text(StatusCode::NOT_FOUND, "api_error_files_disabled");
    }

    let name = form.name.clone();

    let Ok(Some(found)) = storage::read(&db, &name).await else {
        return Response::text(StatusCode::NOT_FOUND, "api_error_storage_not_found");
    };

    // Mirrors `routers::files::ops::storages::remove`.
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
        Ok(_) => Response::new(StatusCode::NO_CONTENT)
            .header("HX-Redirect", with_base_path("/ui/files/storages")),
        Err(problem) => {
            tracing::error!("UI delete dynamic storage failed: {problem}");
            Response::text(StatusCode::INTERNAL_SERVER_ERROR, "api_error_internal")
        }
    }
}

// --- POST /ui/files/delete-file ---

#[post("/ui/files/delete-file")]
pub async fn delete_file(
    gate: ManageGate,
    Form(form): Form<DeleteFileForm>,
    Inject(db): Inject<Db>,
) -> Response {
    if let Err(response) = gate.or_response() {
        return response;
    }
    if !crate::routers::files_enabled() {
        return Response::text(StatusCode::NOT_FOUND, "api_error_files_disabled");
    }

    let storage_name = form.storage.clone();
    let path = form.path.clone();

    if crate::routers::files::relative(&path).is_err() {
        return Response::text(StatusCode::BAD_REQUEST, "api_error_invalid_path");
    }

    // Dynamic storage: the domain layer owns the blob ref-count and quota.
    if storage::read(&db, &storage_name)
        .await
        .ok()
        .flatten()
        .is_some()
    {
        let Some(root) = dynamic::root() else {
            return Response::text(
                StatusCode::INTERNAL_SERVER_ERROR,
                "api_error_no_dynamic_root",
            );
        };
        return match storage_file::delete_file(&db, &storage_name, &path, |sha256| {
            dynamic::blob_path(&root, sha256)
        })
        .await
        {
            Ok(true) => redirect_to_browse(&storage_name, parent_dir(&path)),
            Ok(false) => Response::text(StatusCode::NOT_FOUND, "api_error_file_not_found"),
            Err(problem) => {
                tracing::error!("UI delete dynamic file failed: {problem}");
                Response::text(StatusCode::INTERNAL_SERVER_ERROR, "api_error_internal")
            }
        };
    }

    // Static storage: a plain unlink, confined to the storage root.
    let Some(storage) = crate::routers::files::storage(&storage_name) else {
        return Response::text(StatusCode::NOT_FOUND, "api_error_storage_not_found");
    };
    let Ok(target) = crate::routers::files::resolve(storage, &path) else {
        return Response::text(StatusCode::BAD_REQUEST, "api_error_invalid_path");
    };
    if !crate::routers::files::confined(&storage.root, &target).await {
        return Response::text(StatusCode::FORBIDDEN, "api_error_path_escapes_storage");
    }
    match tokio::fs::remove_file(&target).await {
        Ok(()) => redirect_to_browse(&storage_name, parent_dir(&path)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            Response::text(StatusCode::NOT_FOUND, "api_error_file_not_found")
        }
        Err(err) => {
            tracing::error!("UI delete static file failed: {err}");
            Response::text(StatusCode::INTERNAL_SERVER_ERROR, "api_error_internal")
        }
    }
}

// --- Helpers ---

/// A checkbox is only submitted when checked, so any value present means on.
fn checkbox_on(value: &Option<String>) -> bool {
    value.as_deref().is_some_and(|v| !v.is_empty())
}

/// Parses `raw` in units of `scale` bytes; blank -> `None`, non-negative -> `Some`, else `Err`.
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

fn redirect_to_storage(name: &str) -> Response {
    Response::new(StatusCode::NO_CONTENT).header(
        "HX-Redirect",
        with_base_path(&format!("/ui/files/storages?storage={name}")),
    )
}

/// After a delete, land back on the file browser at the deleted file's parent
/// directory - the browser is the only page that offers the delete now.
fn redirect_to_browse(storage: &str, path: String) -> Response {
    Response::new(StatusCode::NO_CONTENT).header(
        "HX-Redirect",
        with_base_path(&format!(
            "/ui/files/browse?storage={}&path={}",
            encode_query_component(storage),
            encode_query_component(&path)
        )),
    )
}

/// The directory a `/`-separated path sits in, or `""` for a top-level file.
fn parent_dir(path: &str) -> String {
    match path.rsplit_once('/') {
        Some((parent, _)) => parent.to_string(),
        None => String::new(),
    }
}

// --- Rendering ---

pub fn render_storages_page(view: &StoragesView, can_manage: bool) -> Response {
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
        StatusCode::OK,
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

    // The selected name resolved to no storage at all - nothing to show but why.
    if let Some(notice) = selected.notice {
        let mut body = div().class("manage-scroll").child(empty_state(notice));
        if can_manage {
            body = body.child(render_create_form());
        }
        return div()
            .class("h-100 d-flex flex-column")
            .child(title)
            .child(body);
    }

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

    body = body.child(render_browse_link(&selected.name));

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

/// The file list this panel used to carry now lives on its own page, with a
/// real tree and a preview pane - this is the way in.
fn render_browse_link(name: &str) -> Element {
    div().class("mt-4").child(
        a().class("button-neutral-sm")
            .attr(
                "href",
                format!(
                    "{}?storage={}",
                    ui_path("/files/browse"),
                    encode_query_component(name)
                ),
            )
            .child(i().class("fas fa-folder-tree mr-2"))
            .child(
                span()
                    .attr("data-i18n", "ui_browse_open")
                    .text("Browse files"),
            ),
    )
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
        .child(button().class("confirm-modal-backdrop").attr("type", "button").attr("hx-get", ui_path("/files/delete-storage-modal/empty")).attr("hx-target", "#confirm-delete-storage-modal").attr("hx-swap", "outerHTML"))
        .child(
            div()
                .class("confirm-modal-content")
                .child(
                    div()
                        .class("confirm-modal-header")
                        .child(div().class("confirm-modal-title").attr("data-i18n", "ui_storage_delete_title").text("Delete storage"))
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
                        .child(p().attr("data-i18n", "ui_storage_delete_confirm_text").text("Delete this storage and everything in it? This cannot be undone."))
                        .child(div().class("confirm-delete-target").text(name))
                        .child(
                            form()
                                .class("confirm-actions")
                                .attr("hx-post", ui_path("/files/delete-storage"))
                                .attr("hx-swap", "none")
                                .child(input().attr("type", "hidden").attr("name", "name").attr("value", name))
                                .child(
                                    button()
                                        .class("button cancel")
                                        .attr("type", "button")
                                        .attr("hx-get", ui_path("/files/delete-storage-modal/empty"))
                                        .attr("hx-target", "#confirm-delete-storage-modal")
                                        .attr("hx-swap", "outerHTML")
                                        .attr("data-i18n", "ui_common_cancel")
                                        .text("Cancel"),
                                )
                                .child(button().class("button delete").attr("type", "submit").attr("data-i18n", "ui_common_delete").text("Delete")),
                        ),
                ),
        )
        .render()
}

fn empty_delete_storage_modal_element() -> Element {
    div().attr("id", "confirm-delete-storage-modal")
}

// --- Small element helpers ---

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

pub fn register_routes() {
    let _ = files_storages as fn(_, _, _, _) -> _;
    let _ = files_storages_slash as fn(_, _, _, _) -> _;
    let _ = create_storage as fn(_, _, _) -> _;
    let _ = edit_storage as fn(_, _, _, _) -> _;
    let _ = delete_storage_modal as fn(_, _) -> _;
    let _ = empty_delete_storage_modal as fn(_) -> _;
    let _ = delete_storage as fn(_, _, _) -> _;
    let _ = delete_file as fn(_, _, _) -> _;
}
