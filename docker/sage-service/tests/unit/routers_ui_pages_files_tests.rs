//! Unit tests for `routers/ui/pages/files.rs`.

use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode, Uri};
use http_body_util::BodyExt;
use quench_auth::domain::jwt::JwtConfig;
use quench_db::InMemoryDb;
use quench_db::prelude::{Crud, Db};
use quench_http::di::ContainerBuilder;
use quench_http::endpoint::Endpoint;
use quench_http::request::Request;
use sage_service::clients::switchboard::SwitchboardClient;
use sage_service::clients::vllm::VllmClient;
use sage_service::domain::models::File;
use sage_service::routers::ui::pages::files::*;
use std::sync::Arc;

fn db() -> Db {
    Db::InMemory(InMemoryDb::new())
}

/// `attach`/`reprocess` construct `SwitchboardClient`/`VllmClient`, which
/// panic without these env vars set - every test in this binary that
/// tolerates the same fixed values sets them idempotently rather than
/// coordinating through a lock (see `docker/gatehouse-service/src/crypto.rs`'s
/// `TEST_KEY_MATERIAL` convention for why concurrent identical writes are
/// safe here). Matches `routers_files_tests.rs`'s `ensure_switchboard_env`.
fn ensure_switchboard_env() {
    envmnt::set("GATEHOUSE_URL", "http://127.0.0.1:1");
    envmnt::set("CLIENT_SECRET_SAGE_SWITCHBOARD", "test-secret");
    envmnt::set("SWITCHBOARD_URL", "http://127.0.0.1:1");
}

async fn app(db: Db) -> (Arc<dyn Endpoint>, Arc<quench_http::di::Container>) {
    ensure_switchboard_env();
    register_routes();
    let container = ContainerBuilder::new()
        .provide(db)
        .provide(JwtConfig::for_tests())
        .provide(SwitchboardClient::new())
        .provide(VllmClient::new())
        .build()
        .await
        .unwrap();
    (
        quench_starter::http::discover_and_mount("/"),
        Arc::new(container),
    )
}

fn req(method: Method, path: &str, container: &Arc<quench_http::di::Container>) -> Request {
    Request::new(
        method,
        path.parse::<Uri>().unwrap(),
        HeaderMap::new(),
        quench_http::body::InboundBody::from_bytes(Bytes::new()),
        container.clone(),
    )
}

async fn body_text(resp: quench_http::response::Response) -> String {
    let collected = resp.into_hyper().into_body().collect().await.expect("body");
    String::from_utf8_lossy(&collected.to_bytes()).into_owned()
}

fn file(id: &str, owner: &str, status: &str) -> File {
    File {
        id: id.to_string(),
        owner: owner.to_string(),
        file_name: "notes.txt".to_string(),
        mime_type: "text/plain".to_string(),
        file_size: 1024,
        conversation_id: Some("conv".to_string()),
        project_id: None,
        message_id: None,
        status: status.to_string(),
        error_message: None,
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    }
}

// ---------------------------------------------------------------------------
// Pure rendering helpers
// ---------------------------------------------------------------------------

#[test]
fn render_attachment_chip_shows_retry_only_for_a_failed_staged_file() {
    let mut f = file("f1", "alice", "failed");
    f.error_message = Some("boom".to_string());
    let html = render_attachment_chip(&f, true).render();
    assert!(html.contains("attachment-retry"));
    assert!(html.contains("attachment-remove"));
    assert!(html.contains("boom"));
    assert!(!html.contains("attachment-download"));
}

#[test]
fn render_attachment_chip_shows_download_not_remove_when_not_staged() {
    let f = file("f1", "alice", "ready");
    let html = render_attachment_chip(&f, false).render();
    assert!(html.contains("attachment-download"));
    assert!(!html.contains("attachment-remove"));
    assert!(!html.contains("attachment-retry"));
}

#[test]
fn render_attachment_chip_polls_only_while_staged_and_in_progress() {
    let queued = file("f1", "alice", "uploaded");
    assert!(
        render_attachment_chip(&queued, true)
            .render()
            .contains("hx-trigger")
    );
    assert!(
        !render_attachment_chip(&queued, false)
            .render()
            .contains("hx-trigger")
    );

    let ready = file("f2", "alice", "ready");
    assert!(
        !render_attachment_chip(&ready, true)
            .render()
            .contains("hx-trigger")
    );
}

#[test]
fn render_attachment_chip_shows_a_thumbnail_for_images() {
    let mut f = file("f1", "alice", "ready");
    f.mime_type = "image/png".to_string();
    let html = render_attachment_chip(&f, true).render();
    assert!(html.contains("attachment-thumb"));
}

#[test]
fn render_project_file_row_hides_the_status_badge_only_when_ready() {
    let ready = file("f1", "alice", "ready");
    assert!(
        !render_project_file_row(&ready)
            .render()
            .contains("attachment-status")
    );

    let processing = file("f2", "alice", "processing");
    assert!(
        render_project_file_row(&processing)
            .render()
            .contains("attachment-status")
    );
}

#[test]
fn render_project_files_section_shows_the_empty_state_with_no_files() {
    let html = render_project_files_section(&[]).render();
    assert!(html.contains("ui_files_empty_project"));
    assert!(html.contains(">0<"));
}

#[test]
fn render_project_files_section_lists_every_file_and_its_count() {
    let files = vec![file("f1", "alice", "ready"), file("f2", "alice", "ready")];
    let html = render_project_files_section(&files).render();
    assert!(html.contains(">2<"));
    assert!(html.contains("file-item-f1"));
    assert!(html.contains("file-item-f2"));
}

#[test]
fn render_attachments_row_is_none_for_no_files() {
    assert!(render_attachments_row(&[]).is_none());
}

#[test]
fn render_attachments_row_is_some_with_every_file_as_a_readonly_chip() {
    let files = vec![file("f1", "alice", "ready")];
    let row = render_attachments_row(&files).expect("some row").render();
    assert!(row.contains("chip-f1"));
    assert!(row.contains("attachment-download"));
}

// ---------------------------------------------------------------------------
// load_owned_files
// ---------------------------------------------------------------------------

#[tokio::test]
async fn load_owned_files_skips_ids_the_caller_does_not_own_or_that_do_not_exist() {
    let db = db();
    db.repository::<File>()
        .create(&file("mine", "alice", "ready"))
        .await
        .unwrap();
    db.repository::<File>()
        .create(&file("theirs", "bob", "ready"))
        .await
        .unwrap();

    let owned = load_owned_files(
        &db,
        &[
            "mine".to_string(),
            "theirs".to_string(),
            "missing".to_string(),
        ],
        "alice",
    )
    .await;
    assert_eq!(owned.len(), 1);
    assert_eq!(owned[0].id, "mine");
}

// ---------------------------------------------------------------------------
// detach
// ---------------------------------------------------------------------------

#[tokio::test]
async fn detach_removes_a_staged_file_owned_by_the_caller() {
    let db = db();
    db.repository::<File>()
        .create(&file("f1", "admin", "uploaded"))
        .await
        .unwrap();

    let (app, container) = app(db.clone()).await;
    let resp = app
        .call(req(Method::POST, "/ui/files/detach/f1", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(db.repository::<File>().read("f1").await.unwrap().is_none());
}

#[tokio::test]
async fn detach_leaves_a_file_already_linked_to_a_message_alone() {
    let db = db();
    let mut f = file("f1", "admin", "uploaded");
    f.message_id = Some("m1".to_string());
    db.repository::<File>().create(&f).await.unwrap();

    let (app, container) = app(db.clone()).await;
    app.call(req(Method::POST, "/ui/files/detach/f1", &container))
        .await;
    assert!(db.repository::<File>().read("f1").await.unwrap().is_some());
}

// ---------------------------------------------------------------------------
// chip_status
// ---------------------------------------------------------------------------

#[tokio::test]
async fn chip_status_returns_the_rendered_chip_for_an_owned_file() {
    let db = db();
    db.repository::<File>()
        .create(&file("f1", "admin", "processing"))
        .await
        .unwrap();

    let (app, container) = app(db).await;
    let resp = app
        .call(req(Method::GET, "/ui/files/chip/f1", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_text(resp).await;
    assert!(body.contains("chip-f1"));
}

#[tokio::test]
async fn chip_status_is_forbidden_for_a_file_owned_by_someone_else() {
    let db = db();
    db.repository::<File>()
        .create(&file("f1", "someone-else", "ready"))
        .await
        .unwrap();

    let (app, container) = app(db).await;
    let resp = app
        .call(req(Method::GET, "/ui/files/chip/f1", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn chip_status_returns_an_empty_body_when_the_file_is_gone() {
    let (app, container) = app(db()).await;
    let resp = app
        .call(req(
            Method::GET,
            "/ui/files/chip/does-not-exist",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_text(resp).await;
    assert!(body.is_empty());
}

// ---------------------------------------------------------------------------
// delete_modal / delete_file_ui
// ---------------------------------------------------------------------------

#[tokio::test]
async fn delete_modal_names_the_owned_file() {
    let db = db();
    db.repository::<File>()
        .create(&file("f1", "admin", "ready"))
        .await
        .unwrap();

    let (app, container) = app(db).await;
    let resp = app
        .call(req(Method::GET, "/ui/files/delete-modal/f1", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_text(resp).await;
    assert!(body.contains("notes.txt"));
}

#[tokio::test]
async fn delete_modal_is_not_found_for_a_missing_file() {
    let (app, container) = app(db()).await;
    let resp = app
        .call(req(
            Method::GET,
            "/ui/files/delete-modal/missing",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn delete_file_ui_deletes_an_owned_file_and_returns_an_oob_removal() {
    let db = db();
    db.repository::<File>()
        .create(&file("f1", "admin", "ready"))
        .await
        .unwrap();

    let (app, container) = app(db.clone()).await;
    let resp = app
        .call(req(Method::POST, "/ui/files/delete/f1", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(db.repository::<File>().read("f1").await.unwrap().is_none());

    let html = body_text(resp).await;
    assert!(html.contains("hx-swap-oob"));
    assert!(html.contains("file-item-f1"));
}

#[tokio::test]
async fn delete_file_ui_is_forbidden_for_a_file_owned_by_someone_else() {
    let db = db();
    db.repository::<File>()
        .create(&file("f1", "someone-else", "ready"))
        .await
        .unwrap();

    let (app, container) = app(db.clone()).await;
    let resp = app
        .call(req(Method::POST, "/ui/files/delete/f1", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    assert!(db.repository::<File>().read("f1").await.unwrap().is_some());
}
