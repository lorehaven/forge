//! `routers/files.rs` over HTTP: the multipart upload route, its validation, and listing files by
//! conversation or project. Storing a file needs Postgres, so an accepted upload ends in the
//! "Postgres required" refusal here - everything before that point is what these tests exercise.

use crate::env_support::env_lock;
use quench_auth::domain::jwt::JwtConfig;
use quench_core::bytes::Bytes;
use quench_db::InMemoryDb;
use quench_db::prelude::{Crud, Db};
use quench_http::di::ContainerBuilder;
use quench_http::endpoint::Endpoint;
use quench_http::http::{HeaderMap, Method, StatusCode, Uri};
use quench_http::request::Request;
use sage_service::clients::switchboard::SwitchboardClient;
use sage_service::clients::vllm::VllmClient;
use sage_service::domain::models::{Conversation, File, Project};
use std::sync::Arc;

type Container = Arc<quench_http::di::Container>;

async fn app(jwt: Option<JwtConfig>, db: Db) -> (Arc<dyn Endpoint>, Container) {
    sage_service::routers::files::register_routes();
    let mut builder = ContainerBuilder::new()
        .provide(db)
        .provide(SwitchboardClient::new())
        .provide(VllmClient::new());
    if let Some(jwt) = jwt {
        builder = builder.provide(jwt);
    }
    (
        quench_starter::http::discover_and_mount("/"),
        Arc::new(builder.build().await.unwrap()),
    )
}

fn db() -> Db {
    Db::InMemory(InMemoryDb::new())
}

fn get(path: &str, container: &Container) -> Request {
    Request::new(
        Method::GET,
        path.parse::<Uri>().unwrap(),
        HeaderMap::new(),
        quench_http::body::InboundBody::from_bytes(Bytes::new()),
        container.clone(),
    )
}

/// A multipart body: the text `fields`, then `file` as `(file name, bytes)` when given.
fn upload(fields: &[(&str, &str)], file: Option<(&str, &[u8])>, container: &Container) -> Request {
    upload_to("/api/v1/files", fields, file, container)
}

fn upload_to(
    path: &str,
    fields: &[(&str, &str)],
    file: Option<(&str, &[u8])>,
    container: &Container,
) -> Request {
    let boundary = "testboundary";
    let mut body: Vec<u8> = Vec::new();
    for (name, value) in fields {
        body.extend(
            format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n"
            )
            .as_bytes(),
        );
    }
    if let Some((file_name, bytes)) = file {
        body.extend(
            format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{file_name}\"\r\nContent-Type: application/octet-stream\r\n\r\n"
            )
            .as_bytes(),
        );
        body.extend(bytes);
        body.extend(b"\r\n");
    }
    body.extend(format!("--{boundary}--\r\n").as_bytes());

    let mut headers = HeaderMap::new();
    headers.insert(
        "content-type",
        format!("multipart/form-data; boundary={boundary}")
            .parse()
            .unwrap(),
    );
    Request::new(
        Method::POST,
        path.parse::<Uri>().unwrap(),
        headers,
        quench_http::body::InboundBody::from_bytes(Bytes::from(body)),
        container.clone(),
    )
}

async fn text(resp: quench_http::response::Response) -> String {
    let collected = resp
        .into_inner()
        .body
        .collect_limited(usize::MAX)
        .await
        .expect("body");
    String::from_utf8_lossy(&collected).into_owned()
}

async fn seed_conversation(db: &Db, id: &str, owner: &str, project: Option<&str>) {
    db.repository::<Conversation>()
        .create(&Conversation {
            id: id.to_string(),
            title: "t".to_string(),
            active_message_id: None,
            owner: owner.to_string(),
            project_id: project.map(str::to_string),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        })
        .await
        .unwrap();
}

async fn seed_project(db: &Db, id: &str, owner: &str) {
    db.repository::<Project>()
        .create(&Project {
            id: id.to_string(),
            name: id.to_string(),
            owner: owner.to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        })
        .await
        .unwrap();
}

async fn seed_file(db: &Db, id: &str, conversation: Option<&str>, project: Option<&str>) {
    db.repository::<File>()
        .create(&File {
            id: id.to_string(),
            owner: "admin".to_string(),
            file_name: format!("{id}.txt"),
            mime_type: "text/plain".to_string(),
            file_size: 5,
            conversation_id: conversation.map(str::to_string),
            project_id: project.map(str::to_string),
            message_id: None,
            status: "ready".to_string(),
            error_message: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        })
        .await
        .unwrap();
}

#[quench_test::test]
async fn an_upload_without_a_session_is_unauthorized() {
    let mut jwt = JwtConfig::for_tests();
    jwt.auth_enabled = true;
    let (app, container) = app(Some(jwt), db()).await;
    let resp = app
        .call(upload(
            &[("conversation_id", "c1")],
            Some(("a.txt", b"hi")),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // a service without a JWT configuration cannot tell who is asking
    let (app, container) = app_without_jwt().await;
    let resp = app
        .call(get("/api/v1/files?conversation_id=c1", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

async fn app_without_jwt() -> (Arc<dyn Endpoint>, Container) {
    app(None, db()).await
}

#[quench_test::test]
async fn an_upload_is_checked_before_anything_is_stored() {
    let db = db();
    seed_conversation(&db, "mine", "admin", None).await;
    seed_conversation(&db, "theirs", "mallory", None).await;
    let (app, container) = app(Some(JwtConfig::for_tests()), db).await;

    let call = |fields: Vec<(&'static str, &'static str)>,
                file: Option<(&'static str, &'static [u8])>| {
        let app = app.clone();
        let container = container.clone();
        async move {
            let resp = app.call(upload(&fields, file, &container)).await;
            (resp.status(), text(resp).await)
        }
    };

    // no file part at all
    let (status, body) = call(vec![("conversation_id", "mine")], None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body.contains("api_error_missing_file_name"), "{body}");
    // no scope, or two scopes
    let (status, body) = call(vec![], Some(("a.txt", b"hi"))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body.contains("api_error_file_scope_required"), "{body}");
    let (status, _) = call(
        vec![("conversation_id", "mine"), ("project_id", "p")],
        Some(("a.txt", b"hi")),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    // an extension that is not accepted, and an empty file
    let (status, body) = call(vec![("conversation_id", "mine")], Some(("a.exe", b"hi"))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body.contains("api_error_unsupported_file_type"), "{body}");
    let (status, body) = call(vec![("conversation_id", "mine")], Some(("a.txt", b""))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body.contains("api_error_file_empty"), "{body}");
    // someone else's conversation, a missing one, a missing project
    let (status, _) = call(vec![("conversation_id", "theirs")], Some(("a.txt", b"hi"))).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, body) = call(vec![("conversation_id", "none")], Some(("a.txt", b"hi"))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body.contains("api_error_conversation_not_found"), "{body}");
    let (status, body) = call(vec![("project_id", "none")], Some(("a.txt", b"hi"))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body.contains("api_error_project_not_found"), "{body}");
    // everything fine: it is only the storage that is missing
    let (status, body) = call(vec![("conversation_id", "mine")], Some(("a.txt", b"hi"))).await;
    assert_eq!(status, StatusCode::NOT_IMPLEMENTED);
    assert!(body.contains("api_error_postgres_required"), "{body}");
    // unknown multipart fields are ignored
    let (status, _) = call(
        vec![("conversation_id", "mine"), ("note", "ignored")],
        Some(("a.txt", b"hi")),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_IMPLEMENTED);
}

#[allow(clippy::await_holding_lock)] // env vars are process-global
#[quench_test::test]
async fn an_upload_over_the_size_limit_is_refused() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let db = db();
    seed_conversation(&db, "mine", "admin", None).await;
    let (app, container) = app(Some(JwtConfig::for_tests()), db).await;

    quench_core::env::set("SAGE_FILE_MAX_SIZE_MB", "1");
    let big = vec![b'x'; 1024 * 1024 + 1];
    let resp = app
        .call(upload(
            &[("conversation_id", "mine")],
            Some(("big.txt", &big)),
            &container,
        ))
        .await;
    let status = resp.status();
    let body = text(resp).await;
    quench_core::env::remove("SAGE_FILE_MAX_SIZE_MB");
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert!(body.contains("api_error_file_too_large"), "{body}");
}

#[quench_test::test]
async fn files_are_listed_by_conversation_with_the_projects_files_included() {
    let db = db();
    seed_project(&db, "p1", "admin").await;
    seed_conversation(&db, "in-p1", "admin", Some("p1")).await;
    seed_conversation(&db, "sibling", "admin", Some("p1")).await;
    seed_conversation(&db, "loose", "admin", None).await;
    seed_conversation(&db, "theirs", "mallory", None).await;
    seed_file(&db, "own", Some("in-p1"), None).await;
    seed_file(&db, "shared", None, Some("p1")).await;
    seed_file(&db, "sibling-file", Some("sibling"), None).await;
    seed_file(&db, "unrelated", Some("loose"), None).await;
    let (app, container) = app(Some(JwtConfig::for_tests()), db).await;

    let list = |query: &'static str| {
        let app = app.clone();
        let container = container.clone();
        async move {
            let resp = app
                .call(get(&format!("/api/v1/files?{query}"), &container))
                .await;
            (resp.status(), text(resp).await)
        }
    };

    let (status, body) = list("conversation_id=in-p1").await;
    assert_eq!(status, StatusCode::OK);
    for visible in ["own", "shared", "sibling-file"] {
        assert!(
            body.contains(&format!("\"{visible}\"")),
            "{visible} in {body}"
        );
    }
    assert!(!body.contains("unrelated"), "{body}");

    let (_, body) = list("conversation_id=loose").await;
    assert!(
        body.contains("unrelated") && !body.contains("shared"),
        "{body}"
    );

    assert_eq!(
        list("conversation_id=theirs").await.0,
        StatusCode::FORBIDDEN
    );
    let (status, body) = list("conversation_id=none").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body.contains("api_error_conversation_not_found"), "{body}");
}

#[quench_test::test]
async fn files_are_listed_by_project_and_a_scope_is_required() {
    let db = db();
    seed_project(&db, "p1", "admin").await;
    seed_project(&db, "p2", "mallory").await;
    seed_conversation(&db, "in-p1", "admin", Some("p1")).await;
    seed_file(&db, "direct", None, Some("p1")).await;
    seed_file(&db, "via-conversation", Some("in-p1"), None).await;
    seed_file(&db, "elsewhere", None, Some("p2")).await;
    let (app, container) = app(Some(JwtConfig::for_tests()), db).await;

    let list = |query: &'static str| {
        let app = app.clone();
        let container = container.clone();
        async move {
            let resp = app
                .call(get(&format!("/api/v1/files?{query}"), &container))
                .await;
            (resp.status(), text(resp).await)
        }
    };

    let (status, body) = list("project_id=p1").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("\"direct\"") && body.contains("\"via-conversation\""),
        "{body}"
    );
    assert!(!body.contains("elsewhere"), "{body}");
    assert_eq!(list("project_id=p2").await.0, StatusCode::FORBIDDEN);
    let (status, body) = list("project_id=none").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body.contains("api_error_project_not_found"), "{body}");

    for query in ["", "conversation_id=a&project_id=b"] {
        let (status, body) = list(query).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{query}");
        assert!(body.contains("api_error_file_scope_required"), "{body}");
    }
}

// ---------------------------------------------------------------------------
// the composer's upload: `POST /api/v1/files/staged`
// ---------------------------------------------------------------------------

#[quench_test::test]
async fn a_staged_upload_needs_a_conversation_id() {
    let (app, container) = app(Some(JwtConfig::for_tests()), db()).await;
    let resp = app
        .call(upload_to(
            "/api/v1/files/staged",
            &[],
            Some(("a.txt", b"hello")),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    assert!(
        text(resp)
            .await
            .contains("api_error_missing_conversation_id")
    );
}

#[quench_test::test]
async fn a_staged_upload_creates_the_fresh_conversation_before_storing_the_file() {
    let db = db();
    let (app, container) = app(Some(JwtConfig::for_tests()), db.clone()).await;
    let resp = app
        .call(upload_to(
            "/api/v1/files/staged",
            &[("conversation_id", "fresh"), ("project_id", "proj")],
            Some(("a.txt", b"hello")),
            &container,
        ))
        .await;
    // the file itself needs Postgres, which these tests do not have; the conversation was made first
    assert_eq!(resp.status(), StatusCode::NOT_IMPLEMENTED);
    let conv = db
        .repository::<Conversation>()
        .read("fresh")
        .await
        .unwrap()
        .expect("the conversation");
    assert_eq!(conv.title, "", "blank until the first message gives it one");
    assert_eq!(conv.owner, "admin");
    assert_eq!(conv.project_id.as_deref(), Some("proj"));
}

#[quench_test::test]
async fn a_staged_upload_is_forbidden_for_someone_elses_conversation() {
    let db = db();
    seed_conversation(&db, "theirs", "mallory", None).await;
    let (app, container) = app(Some(JwtConfig::for_tests()), db).await;
    let resp = app
        .call(upload_to(
            "/api/v1/files/staged",
            &[("conversation_id", "theirs")],
            Some(("a.txt", b"hello")),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[quench_test::test]
async fn a_staged_upload_checks_the_file_like_any_other() {
    let db = db();
    let (app, container) = app(Some(JwtConfig::for_tests()), db).await;
    for (name, bytes, code) in [
        ("a.exe", &b"hello"[..], "api_error_unsupported_file_type"),
        ("a.txt", &b""[..], "api_error_file_empty"),
    ] {
        let resp = app
            .call(upload_to(
                "/api/v1/files/staged",
                &[("conversation_id", "c1")],
                Some((name, bytes)),
                &container,
            ))
            .await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST, "{name}");
        assert!(text(resp).await.contains(code), "{name}");
    }
}
