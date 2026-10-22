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
    db.execute("TRUNCATE daybook.tasks, daybook.projects, daybook.labels CASCADE")
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

async fn create_label(
    app: &Arc<dyn Endpoint>,
    container: &Arc<Container>,
    user: &str,
    name: &str,
) -> Value {
    let response = app
        .call(request(
            Method::POST,
            "/api/v1/labels",
            Some(json!({"name":name,"color":"#AA00FF"})),
            user,
            container,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    body(response).await
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn reminders_and_labels_round_trip_and_filter_across_lists() {
    let Some((db, _guard)) = database().await else {
        return eprintln!("skipping labels and reminders: DAYBOOK_TEST_DATABASE_URL is not set");
    };
    let (app, container) = app(db).await;

    let errand = create_label(&app, &container, ALICE, "errand").await;
    assert_eq!(errand["color"], "#aa00ff", "colours are stored lowercase");
    let urgent = create_label(&app, &container, ALICE, "Urgent").await;
    let errand_id = errand["id"].as_str().unwrap();
    let urgent_id = urgent["id"].as_str().unwrap();

    // Labels list in name order, ignoring case.
    let labels = get_alice(&app, &container, "/api/v1/labels").await;
    assert_eq!(labels[0]["name"], "errand");
    assert_eq!(labels[1]["name"], "Urgent");

    let project = create_project(&app, &container, ALICE, "Home").await;
    let project_id = project["id"].as_str().unwrap();
    let response = send_alice(
        &app,
        &container,
        Method::POST,
        "/api/v1/tasks",
        Some(json!({
            "content":"Post parcel",
            "remind_at":"2026-11-01T08:30:00Z",
            "due_at":"2026-11-01T12:00:00Z",
            "label_ids":[urgent_id, errand_id, errand_id]
        })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let task = body(response).await;
    assert_eq!(task["remind_at"], "2026-11-01T08:30:00Z");
    assert_eq!(
        task["label_ids"],
        json!([errand_id, urgent_id]),
        "duplicates collapse; ids come back in label-name order"
    );
    let task_id = task["id"].as_str().unwrap();

    let in_project = create_task(&app, &container, ALICE, "Buy bulbs", Some(project_id)).await;
    assert_eq!(in_project["label_ids"], json!([]));
    assert_eq!(in_project["remind_at"], Value::Null);
    let response = send_alice(
        &app,
        &container,
        Method::PUT,
        &format!("/api/v1/tasks/{}", in_project["id"].as_str().unwrap()),
        Some(json!({
            "content":"Buy bulbs",
            "project_id":project_id,
            "label_ids":[errand_id]
        })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    // A label filter spans the inbox and every project...
    let by_label = get_alice(
        &app,
        &container,
        &format!("/api/v1/tasks?label_id={errand_id}"),
    )
    .await;
    assert_eq!(by_label.as_array().unwrap().len(), 2);
    // ...and can still be narrowed to one list.
    let narrowed = get_alice(
        &app,
        &container,
        &format!("/api/v1/tasks?label_id={errand_id}&project_id={project_id}"),
    )
    .await;
    assert_eq!(narrowed.as_array().unwrap().len(), 1);
    let only_urgent = get_alice(
        &app,
        &container,
        &format!("/api/v1/tasks?label_id={urgent_id}"),
    )
    .await;
    assert_eq!(only_urgent.as_array().unwrap().len(), 1);

    // `all=true` spans every list with no label needed.
    let everything = get_alice(&app, &container, "/api/v1/tasks?all=true").await;
    assert_eq!(everything.as_array().unwrap().len(), 2);
    let open_only = get_alice(&app, &container, "/api/v1/tasks?all=true&completed=true").await;
    assert!(open_only.as_array().unwrap().is_empty());

    // PUT replaces: omitting the labels and reminder clears them.
    let response = send_alice(
        &app,
        &container,
        Method::PUT,
        &format!("/api/v1/tasks/{task_id}"),
        Some(json!({"content":"Post parcel"})),
    )
    .await;
    let cleared = body(response).await;
    assert_eq!(cleared["label_ids"], json!([]));
    assert_eq!(cleared["remind_at"], Value::Null);

    // Renaming a label keeps it attached; deleting one detaches it but keeps the task.
    let response = send_alice(
        &app,
        &container,
        Method::PUT,
        &format!("/api/v1/labels/{errand_id}"),
        Some(json!({"name":"errands"})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body(response).await["color"], "#888888");
    delete_alice(&app, &container, &format!("/api/v1/labels/{errand_id}")).await;
    let tasks = get_alice(
        &app,
        &container,
        &format!("/api/v1/tasks?project_id={project_id}"),
    )
    .await;
    assert_eq!(tasks.as_array().unwrap().len(), 1);
    assert_eq!(tasks[0]["label_ids"], json!([]));
}

#[tokio::test]
async fn labels_are_private_unique_and_validated() {
    let Some((db, _guard)) = database().await else {
        return eprintln!("skipping label rules: DAYBOOK_TEST_DATABASE_URL is not set");
    };
    let (app, container) = app(db).await;
    let label = create_label(&app, &container, ALICE, "Private").await;
    let label_id = label["id"].as_str().unwrap();

    // Names are unique per user, ignoring case; another user may reuse one.
    let response = send_alice(
        &app,
        &container,
        Method::POST,
        "/api/v1/labels",
        Some(json!({"name":"private"})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    create_label(&app, &container, BOB, "Private").await;

    // Bob sees only his own, cannot edit or delete Alice's, and cannot attach it to a task.
    let response = app
        .call(request(
            Method::GET,
            "/api/v1/labels",
            None,
            BOB,
            &container,
        ))
        .await;
    assert_eq!(body(response).await.as_array().unwrap().len(), 1);
    for method in [Method::PUT, Method::DELETE] {
        let payload = (method == Method::PUT).then(|| json!({"name":"Stolen"}));
        let response = app
            .call(request(
                method,
                &format!("/api/v1/labels/{label_id}"),
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
            Some(json!({"content":"Cross-owner label","label_ids":[label_id]})),
            BOB,
            &container,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    // Colours must be #rrggbb.
    for color in ["red", "#12345", "#12345g", "123456"] {
        let response = send_alice(
            &app,
            &container,
            Method::POST,
            "/api/v1/labels",
            Some(json!({"name":"Colourful","color":color})),
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{color}");
    }
}

/// Creates a repeating task and returns its JSON.
async fn create_repeating(
    app: &Arc<dyn Endpoint>,
    container: &Arc<Container>,
    content: &str,
    due_at: &str,
    remind_at: Option<&str>,
    recurrence: Value,
) -> Value {
    let response = send_alice(
        app,
        container,
        Method::POST,
        "/api/v1/tasks",
        Some(json!({
            "content":content,
            "due_at":due_at,
            "remind_at":remind_at,
            "recurrence":recurrence
        })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    body(response).await
}

/// PUTs `task` back as Alice with `is_completed` and `due_at` as given, keeping everything else -
/// what a client does when its user ticks the checkbox.
async fn put_task(
    app: &Arc<dyn Endpoint>,
    container: &Arc<Container>,
    task: &Value,
    is_completed: bool,
    due_at: &Value,
) -> Value {
    let response = send_alice(
        app,
        container,
        Method::PUT,
        &format!("/api/v1/tasks/{}", task["id"].as_str().unwrap()),
        Some(json!({
            "content":task["content"],
            "is_completed":is_completed,
            "due_at":due_at,
            "remind_at":task["remind_at"],
            "recurrence":task["recurrence"]
        })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    body(response).await
}

// Far-future due dates, so "the first occurrence after now" is the one after the due date no matter
// when the test runs.

#[tokio::test]
async fn completing_a_repeating_task_rolls_it_forward_and_moves_its_reminder() {
    let Some((db, _guard)) = database().await else {
        return eprintln!("skipping recurrence roll: DAYBOOK_TEST_DATABASE_URL is not set");
    };
    let (app, container) = app(db).await;
    let task = create_repeating(
        &app,
        &container,
        "Water plants",
        "2090-03-02T09:00:00Z",
        Some("2090-03-02T08:30:00Z"),
        json!({"unit":"week","interval":1}),
    )
    .await;
    assert_eq!(
        task["recurrence"],
        json!({"unit":"week","interval":1,"weekdays":[],"mode":"fixed","timezone":"UTC"})
    );

    let rolled = put_task(&app, &container, &task, true, &task["due_at"]).await;
    assert_eq!(rolled["is_completed"], false, "it stays open");
    assert_eq!(rolled["due_at"], "2090-03-09T09:00:00Z");
    assert_eq!(
        rolled["remind_at"], "2090-03-09T08:30:00Z",
        "same lead time"
    );
    assert_eq!(rolled["recurrence"], task["recurrence"]);

    // And again from the new due date.
    let again = put_task(&app, &container, &rolled, true, &rolled["due_at"]).await;
    assert_eq!(again["due_at"], "2090-03-16T09:00:00Z");

    // It is still in the open list, once.
    let open = get_alice(&app, &container, "/api/v1/tasks?completed=false").await;
    assert_eq!(open.as_array().unwrap().len(), 1);
    assert_eq!(open[0]["due_at"], "2090-03-16T09:00:00Z");
}

#[tokio::test]
async fn an_ordinary_task_still_completes_normally() {
    let Some((db, _guard)) = database().await else {
        return eprintln!("skipping plain completion: DAYBOOK_TEST_DATABASE_URL is not set");
    };
    let (app, container) = app(db).await;
    let task = create_task(&app, &container, ALICE, "One-off", None).await;
    let done = put_task(&app, &container, &task, true, &Value::Null).await;
    assert_eq!(done["is_completed"], true);
    assert_eq!(done["recurrence"], Value::Null);
}

#[tokio::test]
async fn reopening_a_repeating_task_does_not_roll_it() {
    let Some((db, _guard)) = database().await else {
        return eprintln!("skipping reopen: DAYBOOK_TEST_DATABASE_URL is not set");
    };
    let (app, container) = app(db).await;
    let task = create_repeating(
        &app,
        &container,
        "Daily",
        "2090-03-02T09:00:00Z",
        None,
        json!({"unit":"day","interval":1}),
    )
    .await;
    // Saving without completing leaves the date alone.
    let saved = put_task(&app, &container, &task, false, &task["due_at"]).await;
    assert_eq!(saved["due_at"], "2090-03-02T09:00:00Z");
    assert_eq!(saved["is_completed"], false);
}

#[tokio::test]
async fn a_monthly_task_on_the_31st_is_not_dragged_to_the_28th_for_good() {
    let Some((db, _guard)) = database().await else {
        return eprintln!("skipping month-end anchor: DAYBOOK_TEST_DATABASE_URL is not set");
    };
    let (app, container) = app(db).await;
    let task = create_repeating(
        &app,
        &container,
        "Pay invoices",
        "2090-01-31T12:00:00Z",
        None,
        json!({"unit":"month","interval":1}),
    )
    .await;
    let february = put_task(&app, &container, &task, true, &task["due_at"]).await;
    assert_eq!(february["due_at"], "2090-02-28T12:00:00Z");
    let march = put_task(&app, &container, &february, true, &february["due_at"]).await;
    assert_eq!(
        march["due_at"], "2090-03-31T12:00:00Z",
        "counted from the 31st"
    );

    // A user moving the due date by hand restarts the schedule from there.
    let moved = put_task(
        &app,
        &container,
        &march,
        false,
        &json!("2090-04-15T12:00:00Z"),
    )
    .await;
    let next = put_task(&app, &container, &moved, true, &moved["due_at"]).await;
    assert_eq!(next["due_at"], "2090-05-15T12:00:00Z");
}

#[tokio::test]
async fn the_rule_is_evaluated_in_its_own_time_zone() {
    let Some((db, _guard)) = database().await else {
        return eprintln!("skipping time zone: DAYBOOK_TEST_DATABASE_URL is not set");
    };
    let (app, container) = app(db).await;
    // 09:00 in Warsaw on 2090-10-27 (CEST, UTC+2). Clocks go back on 2090-10-29 (CEST -> CET).
    let task = create_repeating(
        &app,
        &container,
        "Standup prep",
        "2090-10-27T07:00:00Z",
        None,
        json!({"unit":"day","interval":1,"timezone":"Europe/Warsaw"}),
    )
    .await;
    let mut due = task["due_at"].clone();
    let mut current = task;
    // Roll across the clock change; local 09:00 must hold, so UTC moves from 07:00 to 08:00.
    for _ in 0..3 {
        current = put_task(&app, &container, &current, true, &due).await;
        due = current["due_at"].clone();
    }
    assert_eq!(due, "2090-10-30T08:00:00Z");
}

#[tokio::test]
async fn after_completion_counts_from_the_day_it_is_done() {
    let Some((db, _guard)) = database().await else {
        return eprintln!("skipping after-completion: DAYBOOK_TEST_DATABASE_URL is not set");
    };
    let (app, container) = app(db).await;
    let task = create_repeating(
        &app,
        &container,
        "Descale kettle",
        "2090-03-02T09:00:00Z",
        None,
        json!({"unit":"day","interval":3,"mode":"after_completion"}),
    )
    .await;
    let done = put_task(&app, &container, &task, true, &task["due_at"]).await;
    let next: chrono::DateTime<chrono::Utc> = done["due_at"].as_str().unwrap().parse().unwrap();
    let now = chrono::Utc::now();
    // Three days from today at the original 09:00 - not from the (far-future) due date.
    assert!(next > now + chrono::Duration::days(2), "{next}");
    assert!(next < now + chrono::Duration::days(4), "{next}");
    assert_eq!(next.format("%H:%M").to_string(), "09:00");
}

#[tokio::test]
async fn removing_the_rule_makes_it_an_ordinary_task_again() {
    let Some((db, _guard)) = database().await else {
        return eprintln!("skipping rule removal: DAYBOOK_TEST_DATABASE_URL is not set");
    };
    let (app, container) = app(db).await;
    let task = create_repeating(
        &app,
        &container,
        "Was weekly",
        "2090-03-02T09:00:00Z",
        None,
        json!({"unit":"week","interval":1,"weekdays":[1,4]}),
    )
    .await;
    assert_eq!(task["recurrence"]["weekdays"], json!([1, 4]));

    let response = send_alice(
        &app,
        &container,
        Method::PUT,
        &format!("/api/v1/tasks/{}", task["id"].as_str().unwrap()),
        Some(json!({"content":"Was weekly","due_at":"2090-03-02T09:00:00Z"})),
    )
    .await;
    let cleared = body(response).await;
    assert_eq!(cleared["recurrence"], Value::Null);
    let done = put_task(&app, &container, &cleared, true, &cleared["due_at"]).await;
    assert_eq!(done["is_completed"], true);
}

#[tokio::test]
async fn an_all_day_due_date_is_stored_as_a_date_and_rolls_by_dates() {
    let Some((db, _guard)) = database().await else {
        return eprintln!("skipping all-day: DAYBOOK_TEST_DATABASE_URL is not set");
    };
    let (app, container) = app(db).await;

    // Whatever time the client sent, an all-day due date is the date at midnight UTC.
    let response = send_alice(
        &app,
        &container,
        Method::POST,
        "/api/v1/tasks",
        Some(json!({
            "content":"Pay rent",
            "due_at":"2090-03-02T17:45:00Z",
            "due_all_day":true,
            "remind_at":"2090-03-01T08:00:00Z",
            "recurrence":{"unit":"week","interval":1}
        })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let task = body(response).await;
    assert_eq!(task["due_all_day"], true);
    assert_eq!(task["due_at"], "2090-03-02T00:00:00Z");

    // Completing rolls the date, keeps it all-day, and keeps the reminder's distance (16 hours before).
    let put = |task: Value| {
        let (app, container) = (app.clone(), container.clone());
        async move {
            let response = send_alice(
                &app,
                &container,
                Method::PUT,
                &format!("/api/v1/tasks/{}", task["id"].as_str().unwrap()),
                Some(json!({
                    "content":task["content"],
                    "is_completed":true,
                    "due_at":task["due_at"],
                    "due_all_day":task["due_all_day"],
                    "remind_at":task["remind_at"],
                    "recurrence":task["recurrence"]
                })),
            )
            .await;
            assert_eq!(response.status(), StatusCode::OK);
            body(response).await
        }
    };
    let rolled = put(task).await;
    assert_eq!(rolled["is_completed"], false);
    assert_eq!(rolled["due_all_day"], true);
    assert_eq!(rolled["due_at"], "2090-03-09T00:00:00Z");
    assert_eq!(rolled["remind_at"], "2090-03-08T08:00:00Z");
}

#[tokio::test]
async fn all_day_means_nothing_without_a_date_and_can_be_switched_off() {
    let Some((db, _guard)) = database().await else {
        return eprintln!("skipping all-day rules: DAYBOOK_TEST_DATABASE_URL is not set");
    };
    let (app, container) = app(db).await;
    let response = send_alice(
        &app,
        &container,
        Method::POST,
        "/api/v1/tasks",
        Some(json!({"content":"No date","due_all_day":true})),
    )
    .await;
    let task = body(response).await;
    assert_eq!(task["due_all_day"], false);
    assert_eq!(task["due_at"], Value::Null);

    // A timed due date is untouched, and the default for a task that says nothing is "has a time".
    let response = send_alice(
        &app,
        &container,
        Method::PUT,
        &format!("/api/v1/tasks/{}", task["id"].as_str().unwrap()),
        Some(json!({"content":"Timed","due_at":"2090-03-02T17:45:00Z"})),
    )
    .await;
    let timed = body(response).await;
    assert_eq!(timed["due_all_day"], false);
    assert_eq!(timed["due_at"], "2090-03-02T17:45:00Z");
}
