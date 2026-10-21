//! Postgres-backed API behavior. Set `DAYBOOK_TEST_DATABASE_URL` to a throwaway
//! database with the daybook and auth modules installed to run these tests.
#![allow(dead_code)]

use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode, Uri};
use http_body_util::BodyExt;
use quench_auth::domain::jwt::{Claims, JwtConfig};
use quench_db::prelude::{Database, Db};
use quench_http::prelude::{Container, ContainerBuilder, Endpoint, Request};
use serde_json::{Value, json};
use std::sync::{Arc, OnceLock};
use tokio::sync::{Mutex, MutexGuard};

const ALICE: &str = "daybook-test-alice";
const BOB: &str = "daybook-test-bob";

fn lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

async fn database() -> Option<(Db, MutexGuard<'static, ()>)> {
    let url = std::env::var("DAYBOOK_TEST_DATABASE_URL").ok()?;
    if url.trim().is_empty() {
        return None;
    }
    let guard = lock().lock().await;
    let db = Db::connect(&url).await.expect("connect to test database");
    db.execute("TRUNCATE daybook.tasks, daybook.projects CASCADE")
        .await
        .expect("clear test rows");
    db.execute(&format!(
        "INSERT INTO auth.users (username,password,roles) VALUES \
         ('{ALICE}','x','[]'::jsonb),('{BOB}','x','[]'::jsonb) \
         ON CONFLICT (username) DO NOTHING"
    ))
    .await
    .expect("seed users");
    Some((db, guard))
}

async fn app(db: Db) -> (Arc<dyn Endpoint>, Arc<Container>) {
    daybook_service::routers::register_routes();
    let container = ContainerBuilder::new()
        .provide(db)
        .provide(JwtConfig::for_tests())
        .build()
        .await
        .expect("container");
    (
        quench_starter::http::discover_and_mount("/"),
        Arc::new(container),
    )
}

fn request(
    method: Method,
    path: &str,
    payload: Option<Value>,
    user: &str,
    container: &Arc<Container>,
) -> Request {
    let mut headers = HeaderMap::new();
    let bytes = payload.map_or_else(Bytes::new, |value| {
        headers.insert("content-type", "application/json".parse().unwrap());
        Bytes::from(serde_json::to_vec(&value).expect("serialize payload"))
    });
    let mut request = Request::new(
        method,
        path.parse::<Uri>().expect("valid URI"),
        headers,
        quench_http::body::InboundBody::from_bytes(bytes),
        container.clone(),
    );
    request.extensions_mut().insert(Claims::for_audiences(
        user.to_string(),
        vec!["daybook".to_string()],
        "daybook:read".to_string(),
        None,
        3600,
    ));
    request
}

async fn body(response: quench_http::response::Response) -> Value {
    let bytes = response
        .into_hyper()
        .into_body()
        .collect()
        .await
        .expect("response body")
        .to_bytes();
    serde_json::from_slice(&bytes).expect("JSON response")
}

async fn create_project(
    app: &Arc<dyn Endpoint>,
    container: &Arc<Container>,
    user: &str,
    name: &str,
) -> Value {
    let response = app
        .call(request(
            Method::POST,
            "/api/v1/projects",
            Some(json!({"name":name})),
            user,
            container,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    body(response).await
}

async fn create_task(
    app: &Arc<dyn Endpoint>,
    container: &Arc<Container>,
    user: &str,
    content: &str,
    project_id: Option<&str>,
) -> Value {
    let response = app
        .call(request(
            Method::POST,
            "/api/v1/tasks",
            Some(json!({"content":content,"project_id":project_id})),
            user,
            container,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    body(response).await
}

/// Sends `method uri` as Alice and returns the raw response.
async fn send_alice(
    app: &Arc<dyn Endpoint>,
    container: &Arc<Container>,
    method: Method,
    uri: &str,
    payload: Option<Value>,
) -> quench_http::response::Response {
    app.call(request(method, uri, payload, ALICE, container))
        .await
}

/// GETs `uri` as Alice and returns the JSON body.
async fn get_alice(app: &Arc<dyn Endpoint>, container: &Arc<Container>, uri: &str) -> Value {
    body(send_alice(app, container, Method::GET, uri, None).await).await
}

/// DELETEs `uri` as Alice and asserts it answered `204 No Content`.
async fn delete_alice(app: &Arc<dyn Endpoint>, container: &Arc<Container>, uri: &str) {
    let response = send_alice(app, container, Method::DELETE, uri, None).await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn inbox_project_completion_and_crud_round_trip() {
    let Some((db, _guard)) = database().await else {
        return eprintln!("skipping API CRUD: DAYBOOK_TEST_DATABASE_URL is not set");
    };
    let (app, container) = app(db).await;

    let inbox_task = create_task(&app, &container, ALICE, "Pay rent", None).await;
    assert_eq!(inbox_task["project_id"], Value::Null);
    assert_eq!(inbox_task["is_completed"], false);
    let inbox = get_alice(&app, &container, "/api/v1/tasks").await;
    assert_eq!(inbox.as_array().unwrap().len(), 1);

    let project = create_project(&app, &container, ALICE, "Home").await;
    let project_id = project["id"].as_str().unwrap();
    let project_task =
        create_task(&app, &container, ALICE, "Replace a bulb", Some(project_id)).await;
    let by_project = format!("/api/v1/tasks?project_id={project_id}");
    let project_tasks = get_alice(&app, &container, &by_project).await;
    assert_eq!(project_tasks.as_array().unwrap().len(), 1);
    assert_eq!(project_tasks[0]["id"], project_task["id"]);

    let task_id = project_task["id"].as_str().unwrap();
    let response = send_alice(
        &app,
        &container,
        Method::PUT,
        &format!("/api/v1/tasks/{task_id}"),
        Some(json!({
            "content":"Replace both bulbs",
            "description":"Hallway",
            "project_id":project_id,
            "is_completed":true,
            "sort_order":8
        })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let updated = body(response).await;
    assert_eq!(updated["content"], "Replace both bulbs");
    assert_eq!(updated["description"], "Hallway");
    assert_eq!(updated["is_completed"], true);
    assert_eq!(updated["sort_order"], 8);

    let completed = get_alice(&app, &container, &format!("{by_project}&completed=true")).await;
    assert_eq!(completed.as_array().unwrap().len(), 1);

    delete_alice(&app, &container, &format!("/api/v1/tasks/{task_id}")).await;
    delete_alice(&app, &container, &format!("/api/v1/projects/{project_id}")).await;
    let after_delete = get_alice(&app, &container, &by_project).await;
    assert!(after_delete.as_array().unwrap().is_empty());

    // Inbox tasks are independent of the custom project lifecycle.
    let remaining = get_alice(&app, &container, "/api/v1/tasks").await;
    assert_eq!(remaining.as_array().unwrap().len(), 1);
    assert_eq!(remaining[0]["id"], inbox_task["id"]);
}

#[tokio::test]
async fn task_and_project_data_are_isolated_by_gatehouse_subject() {
    let Some((db, _guard)) = database().await else {
        return eprintln!("skipping owner isolation: DAYBOOK_TEST_DATABASE_URL is not set");
    };
    let (app, container) = app(db).await;
    let project = create_project(&app, &container, ALICE, "Private").await;
    let project_id = project["id"].as_str().unwrap();
    let task = create_task(&app, &container, ALICE, "Alice's task", Some(project_id)).await;
    let task_id = task["id"].as_str().unwrap();

    for path in ["/api/v1/tasks", "/api/v1/projects"] {
        let response = app
            .call(request(Method::GET, path, None, BOB, &container))
            .await;
        assert!(
            body(response).await.as_array().unwrap().is_empty(),
            "{path}"
        );
    }

    for method in [Method::PUT, Method::DELETE] {
        let payload =
            (method == Method::PUT).then(|| json!({"content":"Stolen","is_completed":true}));
        let response = app
            .call(request(
                method.clone(),
                &format!("/api/v1/tasks/{task_id}"),
                payload,
                BOB,
                &container,
            ))
            .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    let response = app
        .call(request(
            Method::POST,
            "/api/v1/tasks",
            Some(json!({"content":"Cross-owner link","project_id":project_id})),
            BOB,
            &container,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let response = app
        .call(request(
            Method::GET,
            &format!("/api/v1/tasks?project_id={project_id}"),
            None,
            ALICE,
            &container,
        ))
        .await;
    assert_eq!(body(response).await.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn missing_records_and_invalid_payloads_have_stable_http_statuses() {
    let Some((db, _guard)) = database().await else {
        return eprintln!("skipping API errors: DAYBOOK_TEST_DATABASE_URL is not set");
    };
    let (app, container) = app(db).await;

    for (method, path) in [
        (Method::PUT, "/api/v1/tasks/missing"),
        (Method::DELETE, "/api/v1/tasks/missing"),
        (Method::PUT, "/api/v1/projects/missing"),
        (Method::DELETE, "/api/v1/projects/missing"),
    ] {
        let payload = (method == Method::PUT).then(|| json!({"content":"x","name":"x"}));
        let response = app
            .call(request(method.clone(), path, payload, ALICE, &container))
            .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{method} {path}");
    }

    for (path, payload) in [
        ("/api/v1/tasks", json!({"content":" "})),
        ("/api/v1/projects", json!({"name":" "})),
    ] {
        let response = app
            .call(request(
                Method::POST,
                path,
                Some(payload),
                ALICE,
                &container,
            ))
            .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{path}");
    }
}
