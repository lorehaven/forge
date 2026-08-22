//! Unit tests for `routers/files.rs`.

use actix_multipart::form::bytes::Bytes as MultipartBytes;
use actix_multipart::form::text::Text;
use actix_web::App;
use actix_web::http::StatusCode;
use actix_web::test as actix_test;
use actix_web::web::{Bytes, Data};
use quench_auth::prelude::JwtConfig;
use quench_db::InMemoryDb;
use quench_db::prelude::{Crud, Db};
use sage_service::clients::switchboard::SwitchboardClient;
use sage_service::clients::vllm::VllmClient;
use sage_service::domain::models::{Conversation, File, Project};
use sage_service::routers::files::*;

#[test]
fn extension_lookup_is_case_insensitive_and_uses_the_last_dot() {
    assert_eq!(allowed_mime_type("photo.PNG"), Some("image/png"));
    assert_eq!(allowed_mime_type("archive.tar.gz"), None);
    assert_eq!(allowed_mime_type("notes.backup.md"), Some("text/markdown"));
    assert_eq!(allowed_mime_type("README"), None);
    assert_eq!(allowed_mime_type("evil.exe"), None);
}

#[test]
fn accept_attribute_covers_every_supported_extension() {
    let accept = upload_accept_attribute();
    let listed: Vec<&str> = accept.split(',').collect();
    assert_eq!(listed.len(), ALLOWED_UPLOAD_TYPES.len());
    for (ext, _) in ALLOWED_UPLOAD_TYPES {
        assert!(
            listed.contains(&format!(".{ext}").as_str()),
            "accept filter is missing .{ext}"
        );
    }
    // The formats the picker used to be limited to, plus images.
    for expected in [".pdf", ".txt", ".csv", ".md", ".png", ".jpg", ".webp"] {
        assert!(accept.contains(expected), "accept filter lost {expected}");
    }
}

#[test]
fn no_duplicate_extensions() {
    let mut seen: Vec<&str> = ALLOWED_UPLOAD_TYPES.iter().map(|(ext, _)| *ext).collect();
    let total = seen.len();
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(seen.len(), total, "duplicate extension in the accept table");
}

/// Everything the upload endpoint accepts must have somewhere to go: images
/// bypass extraction, every other MIME type must be one the extractor
/// knows, or the file would be stored only to fail processing.
#[test]
fn every_accepted_mime_is_handled_downstream() {
    for (ext, mime) in ALLOWED_UPLOAD_TYPES {
        if sage_service::files::is_image_mime(mime) {
            continue;
        }
        // Empty/dummy input still fails (no content), but not with the "unsupported type" error.
        if let Err(err) = sage_service::files::extractor::extract_text(mime, b"probe") {
            assert!(
                !err.starts_with("Unsupported MIME type"),
                ".{ext} maps to {mime}, which the extractor rejects: {err}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Handler-level tests
// ---------------------------------------------------------------------------

fn db() -> Db {
    Db::InMemory(InMemoryDb::new())
}

fn ensure_switchboard_env() {
    envmnt::set("GATEHOUSE_URL", "http://127.0.0.1:1");
    envmnt::set("CLIENT_SECRET_SAGE_SWITCHBOARD", "test-secret");
    envmnt::set("SWITCHBOARD_URL", "http://127.0.0.1:1");
}

/// A macro rather than a function: the `Service` type `init_service`
/// returns is unnameable without adding `actix-http` as a direct
/// dependency just for this, so it's built inline where full inference
/// applies instead.
macro_rules! test_app {
    ($db:expr) => {{
        ensure_switchboard_env();
        actix_test::init_service(
            App::new()
                .app_data(Data::new($db))
                .app_data(Data::new(JwtConfig::for_tests()))
                .app_data(Data::new(SwitchboardClient::new()))
                .app_data(Data::new(VllmClient::new()))
                .service(scope()),
        )
        .await
    }};
}

async fn seed_conversation(db: &Db, id: &str, owner: &str, project_id: Option<&str>) {
    db.repository::<Conversation>()
        .create(&Conversation {
            id: id.to_string(),
            title: "t".to_string(),
            active_message_id: None,
            owner: owner.to_string(),
            project_id: project_id.map(str::to_string),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        })
        .await
        .unwrap();
}

async fn seed_project(db: &Db, id: &str, owner: &str) {
    db.repository::<Project>()
        .create(&Project {
            id: id.to_string(),
            name: "p".to_string(),
            owner: owner.to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        })
        .await
        .unwrap();
}

async fn seed_file(db: &Db, id: &str, owner: &str, conversation_id: Option<&str>) -> File {
    let file = File {
        id: id.to_string(),
        owner: owner.to_string(),
        file_name: "notes.txt".to_string(),
        mime_type: "text/plain".to_string(),
        file_size: 5,
        conversation_id: conversation_id.map(str::to_string),
        project_id: None,
        message_id: None,
        status: "uploaded".to_string(),
        error_message: None,
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    };
    db.repository::<File>().create(&file).await.unwrap();
    file
}

#[actix_web::test]
async fn list_files_requires_exactly_one_scope() {
    let app = test_app!(db());
    let req = actix_test::TestRequest::get()
        .uri("/api/v1/files")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[actix_web::test]
async fn list_files_reports_not_found_for_a_missing_conversation() {
    let app = test_app!(db());
    let req = actix_test::TestRequest::get()
        .uri("/api/v1/files?conversation_id=missing")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn list_files_is_forbidden_for_someone_elses_conversation() {
    let db = db();
    seed_conversation(&db, "conv-1", "bob", None).await;
    let app = test_app!(db);
    let req = actix_test::TestRequest::get()
        .uri("/api/v1/files?conversation_id=conv-1")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[actix_web::test]
async fn list_files_returns_the_owners_conversation_files() {
    let db = db();
    seed_conversation(&db, "conv-1", "admin", None).await;
    seed_file(&db, "file-1", "admin", Some("conv-1")).await;
    let app = test_app!(db);
    let req = actix_test::TestRequest::get()
        .uri("/api/v1/files?conversation_id=conv-1")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let files: Vec<File> = actix_test::read_body_json(resp).await;
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].id, "file-1");
}

#[actix_web::test]
async fn list_files_reports_not_found_for_a_missing_project() {
    let app = test_app!(db());
    let req = actix_test::TestRequest::get()
        .uri("/api/v1/files?project_id=missing")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn list_files_is_forbidden_for_someone_elses_project() {
    let db = db();
    seed_project(&db, "proj-1", "bob").await;
    let app = test_app!(db);
    let req = actix_test::TestRequest::get()
        .uri("/api/v1/files?project_id=proj-1")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[actix_web::test]
async fn get_file_returns_not_found_for_a_missing_file() {
    let app = test_app!(db());
    let req = actix_test::TestRequest::get()
        .uri("/api/v1/files/missing")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn get_file_is_forbidden_for_a_file_owned_by_someone_else() {
    let db = db();
    seed_file(&db, "file-1", "bob", None).await;
    let app = test_app!(db);
    let req = actix_test::TestRequest::get()
        .uri("/api/v1/files/file-1")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[actix_web::test]
async fn get_file_returns_the_owners_file() {
    let db = db();
    seed_file(&db, "file-1", "admin", None).await;
    let app = test_app!(db);
    let req = actix_test::TestRequest::get()
        .uri("/api/v1/files/file-1")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[actix_web::test]
async fn delete_file_removes_the_owners_file() {
    let db = db();
    seed_file(&db, "file-1", "admin", None).await;
    let app = test_app!(db.clone());
    let req = actix_test::TestRequest::delete()
        .uri("/api/v1/files/file-1")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    assert!(
        db.repository::<File>()
            .read("file-1")
            .await
            .unwrap()
            .is_none()
    );
}

#[actix_web::test]
async fn delete_file_is_forbidden_for_a_file_owned_by_someone_else() {
    let db = db();
    seed_file(&db, "file-1", "bob", None).await;
    let app = test_app!(db);
    let req = actix_test::TestRequest::delete()
        .uri("/api/v1/files/file-1")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[actix_web::test]
async fn download_file_reports_not_implemented_without_postgres() {
    let db = db();
    seed_file(&db, "file-1", "admin", None).await;
    let app = test_app!(db);
    let req = actix_test::TestRequest::get()
        .uri("/api/v1/files/file-1/download")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_IMPLEMENTED);
}

#[actix_web::test]
async fn list_chunks_returns_not_found_for_a_missing_file() {
    let app = test_app!(db());
    let req = actix_test::TestRequest::get()
        .uri("/api/v1/files/missing/chunks")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn list_chunks_returns_an_empty_list_for_a_file_with_none() {
    let db = db();
    seed_file(&db, "file-1", "admin", None).await;
    let app = test_app!(db);
    let req = actix_test::TestRequest::get()
        .uri("/api/v1/files/file-1/chunks")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let chunks: Vec<serde_json::Value> = actix_test::read_body_json(resp).await;
    assert!(chunks.is_empty());
}

#[actix_web::test]
async fn reprocess_file_conflicts_when_already_processing() {
    let db = db();
    let mut file = seed_file(&db, "file-1", "admin", None).await;
    file.status = sage_service::files::STATUS_PROCESSING.to_string();
    db.repository::<File>().update(&file).await.unwrap();
    let app = test_app!(db);
    let req = actix_test::TestRequest::post()
        .uri("/api/v1/files/file-1/reprocess")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::CONFLICT);
}

#[actix_web::test]
async fn reprocess_file_rejects_images() {
    let db = db();
    let mut file = seed_file(&db, "file-1", "admin", None).await;
    file.mime_type = "image/png".to_string();
    db.repository::<File>().update(&file).await.unwrap();
    let app = test_app!(db);
    let req = actix_test::TestRequest::post()
        .uri("/api/v1/files/file-1/reprocess")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[actix_web::test]
async fn reprocess_file_returns_not_found_for_a_missing_file() {
    let app = test_app!(db());
    let req = actix_test::TestRequest::post()
        .uri("/api/v1/files/missing/reprocess")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn every_handler_is_unauthorized_without_a_claim_when_auth_is_required() {
    // Setting `auth_enabled` on this instance directly (rather than via the
    // `SERVICE_AUTH_ENABLED` env var `JwtConfig::for_tests()` would
    // otherwise read) avoids a process-wide race with every other test in
    // this file, which all expect the default disabled/admin-bypass config.
    ensure_switchboard_env();
    let mut jwt_config = JwtConfig::for_tests();
    jwt_config.auth_enabled = true;
    let app = actix_test::init_service(
        App::new()
            .app_data(Data::new(db()))
            .app_data(Data::new(jwt_config))
            .app_data(Data::new(SwitchboardClient::new()))
            .app_data(Data::new(VllmClient::new()))
            .service(scope()),
    )
    .await;

    for req in [
        actix_test::TestRequest::get()
            .uri("/api/v1/files")
            .to_request(),
        actix_test::TestRequest::get()
            .uri("/api/v1/files/x")
            .to_request(),
        actix_test::TestRequest::delete()
            .uri("/api/v1/files/x")
            .to_request(),
        actix_test::TestRequest::get()
            .uri("/api/v1/files/x/download")
            .to_request(),
        actix_test::TestRequest::get()
            .uri("/api/v1/files/x/chunks")
            .to_request(),
        actix_test::TestRequest::post()
            .uri("/api/v1/files/x/reprocess")
            .to_request(),
    ] {
        let resp = actix_test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }
}

// ---------------------------------------------------------------------------
// `create_uploaded_file` directly - `upload_file`'s handler body, exercised
// without needing a real multipart HTTP body.
// ---------------------------------------------------------------------------

fn upload_form(
    file_name: &str,
    data: &[u8],
    conversation_id: Option<&str>,
    project_id: Option<&str>,
) -> FileUploadForm {
    FileUploadForm {
        file: MultipartBytes {
            data: Bytes::copy_from_slice(data),
            content_type: None,
            file_name: Some(file_name.to_string()),
        },
        conversation_id: conversation_id.map(|c| Text(c.to_string())),
        project_id: project_id.map(|p| Text(p.to_string())),
    }
}

#[actix_web::test]
async fn create_uploaded_file_requires_exactly_one_scope() {
    ensure_switchboard_env();
    let db = db();
    let switchboard = SwitchboardClient::new();
    let vllm = VllmClient::new();
    let form = upload_form("a.txt", b"hello", None, None);

    let err = create_uploaded_file(&db, &switchboard, &vllm, "alice", form)
        .await
        .unwrap_err();
    assert_eq!(err.status(), StatusCode::BAD_REQUEST);
}

#[actix_web::test]
async fn create_uploaded_file_rejects_an_unsupported_extension() {
    ensure_switchboard_env();
    let db = db();
    let switchboard = SwitchboardClient::new();
    let vllm = VllmClient::new();
    let form = upload_form("virus.exe", b"hello", Some("conv-1"), None);

    let err = create_uploaded_file(&db, &switchboard, &vllm, "alice", form)
        .await
        .unwrap_err();
    assert_eq!(err.status(), StatusCode::BAD_REQUEST);
}

#[actix_web::test]
async fn create_uploaded_file_rejects_an_empty_file() {
    ensure_switchboard_env();
    let db = db();
    let switchboard = SwitchboardClient::new();
    let vllm = VllmClient::new();
    let form = upload_form("a.txt", b"", Some("conv-1"), None);

    let err = create_uploaded_file(&db, &switchboard, &vllm, "alice", form)
        .await
        .unwrap_err();
    assert_eq!(err.status(), StatusCode::BAD_REQUEST);
}

#[actix_web::test]
async fn create_uploaded_file_reports_not_found_for_a_missing_conversation() {
    ensure_switchboard_env();
    let db = db();
    let switchboard = SwitchboardClient::new();
    let vllm = VllmClient::new();
    let form = upload_form("a.txt", b"hello", Some("missing"), None);

    let err = create_uploaded_file(&db, &switchboard, &vllm, "alice", form)
        .await
        .unwrap_err();
    assert_eq!(err.status(), StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn create_uploaded_file_is_forbidden_for_someone_elses_conversation() {
    ensure_switchboard_env();
    let db = db();
    seed_conversation(&db, "conv-1", "bob", None).await;
    let switchboard = SwitchboardClient::new();
    let vllm = VllmClient::new();
    let form = upload_form("a.txt", b"hello", Some("conv-1"), None);

    let err = create_uploaded_file(&db, &switchboard, &vllm, "alice", form)
        .await
        .unwrap_err();
    assert_eq!(err.status(), StatusCode::FORBIDDEN);
}

#[actix_web::test]
async fn create_uploaded_file_reports_not_found_for_a_missing_project() {
    ensure_switchboard_env();
    let db = db();
    let switchboard = SwitchboardClient::new();
    let vllm = VllmClient::new();
    let form = upload_form("a.txt", b"hello", None, Some("missing"));

    let err = create_uploaded_file(&db, &switchboard, &vllm, "alice", form)
        .await
        .unwrap_err();
    assert_eq!(err.status(), StatusCode::NOT_FOUND);
}

/// `create_uploaded_file` only writes through `Db::Postgres` - the
/// `InMemory` backend it's given here (deliberately, matching the rest of
/// this file's DB-free tests) reaches the "would insert, but no Postgres"
/// branch, which is `NOT_IMPLEMENTED`. The actual insert path needs a real
/// Postgres and is left uncovered here; see the module docs on
/// `create_uploaded_file` for why `InMemory` is refused rather than
/// silently degraded.
#[actix_web::test]
async fn create_uploaded_file_refuses_to_write_without_postgres() {
    ensure_switchboard_env();
    let db = db();
    seed_conversation(&db, "conv-1", "alice", None).await;
    let switchboard = SwitchboardClient::new();
    let vllm = VllmClient::new();
    let form = upload_form("a.txt", b"hello", Some("conv-1"), None);

    let err = create_uploaded_file(&db, &switchboard, &vllm, "alice", form)
        .await
        .unwrap_err();
    assert_eq!(err.status(), StatusCode::NOT_IMPLEMENTED);
}

// ---------------------------------------------------------------------------
// Free functions with an `InMemory` branch - `visible_files_for_conversation`,
// `visible_files_for_project`, `link_files_to_message`, `files_by_message`.
// Their `Db::Postgres` arms need a real Postgres and are left uncovered here.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn visible_files_for_conversation_includes_files_attached_directly() {
    let db = db();
    seed_conversation(&db, "conv-1", "alice", None).await;
    seed_conversation(&db, "conv-2", "alice", None).await;
    let f1 = seed_file(&db, "f1", "alice", Some("conv-1")).await;
    seed_file(&db, "f2", "alice", Some("conv-2")).await;

    let conversation = db
        .repository::<Conversation>()
        .read("conv-1")
        .await
        .unwrap()
        .unwrap();
    let files = visible_files_for_conversation(&db, &conversation)
        .await
        .unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].id, f1.id);
}

#[tokio::test]
async fn visible_files_for_conversation_includes_files_via_the_shared_project() {
    let db = db();
    seed_project(&db, "proj-1", "alice").await;
    seed_conversation(&db, "conv-1", "alice", Some("proj-1")).await;
    seed_conversation(&db, "conv-2", "alice", Some("proj-1")).await;
    let mut project_file = seed_file(&db, "f1", "alice", None).await;
    project_file.project_id = Some("proj-1".to_string());
    db.repository::<File>().update(&project_file).await.unwrap();

    let conversation = db
        .repository::<Conversation>()
        .read("conv-2")
        .await
        .unwrap()
        .unwrap();
    let files = visible_files_for_conversation(&db, &conversation)
        .await
        .unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].id, "f1");
}

#[tokio::test]
async fn visible_files_for_conversation_is_empty_without_a_project_or_direct_attachment() {
    let db = db();
    seed_conversation(&db, "conv-1", "alice", None).await;
    seed_file(&db, "f1", "alice", Some("other-conv")).await;

    let conversation = db
        .repository::<Conversation>()
        .read("conv-1")
        .await
        .unwrap()
        .unwrap();
    let files = visible_files_for_conversation(&db, &conversation)
        .await
        .unwrap();
    assert!(files.is_empty());
}

#[tokio::test]
async fn visible_files_for_project_includes_files_attached_directly_and_via_conversations() {
    let db = db();
    seed_project(&db, "proj-1", "alice").await;
    seed_conversation(&db, "conv-1", "alice", Some("proj-1")).await;
    let mut direct_file = seed_file(&db, "f1", "alice", None).await;
    direct_file.project_id = Some("proj-1".to_string());
    db.repository::<File>().update(&direct_file).await.unwrap();
    seed_file(&db, "f2", "alice", Some("conv-1")).await;
    seed_file(&db, "f3", "alice", Some("unrelated-conv")).await;

    let files = visible_files_for_project(&db, "proj-1").await.unwrap();
    let mut ids: Vec<&str> = files.iter().map(|f| f.id.as_str()).collect();
    ids.sort_unstable();
    assert_eq!(ids, vec!["f1", "f2"]);
}

#[tokio::test]
async fn link_files_to_message_is_a_no_op_without_postgres() {
    let db = db();
    // `InMemory` short-circuits to `Ok(())` before touching anything - just
    // confirm it doesn't error even with a nonsense id list.
    let result =
        link_files_to_message(&db, &["missing".to_string()], "msg-1", "conv-1", "alice").await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn link_files_to_message_is_a_no_op_for_an_empty_id_list() {
    let db = db();
    let result = link_files_to_message(&db, &[], "msg-1", "conv-1", "alice").await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn files_by_message_is_empty_without_postgres() {
    let db = db();
    let map = files_by_message(&db, &["msg-1".to_string()]).await;
    assert!(map.is_empty());
}

#[tokio::test]
async fn files_by_message_is_empty_for_an_empty_message_id_list() {
    let db = db();
    let map = files_by_message(&db, &[]).await;
    assert!(map.is_empty());
}
