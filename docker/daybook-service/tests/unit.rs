use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode, Uri};
use http_body_util::BodyExt;
use quench_auth::domain::jwt::{Claims, JwtConfig};
use quench_db::{Db, InMemoryDb};
use quench_http::prelude::{Container, ContainerBuilder, Endpoint, Request};
use serde_json::{Value, json};
use std::sync::Arc;

async fn enforcing_app() -> (Arc<dyn Endpoint>, Arc<Container>) {
    daybook_service::routers::register_routes();
    let container = ContainerBuilder::new()
        .provide(Db::InMemory(InMemoryDb::new()))
        .provide(enforcing())
        .build()
        .await
        .expect("container");
    (
        quench_starter::http::discover_and_mount("/"),
        Arc::new(container),
    )
}

async fn app() -> (Arc<dyn Endpoint>, Arc<Container>) {
    daybook_service::routers::register_routes();
    let container = ContainerBuilder::new()
        .provide(Db::InMemory(InMemoryDb::new()))
        .provide(JwtConfig::for_tests())
        .build()
        .await
        .expect("container");
    (
        quench_starter::http::discover_and_mount("/"),
        Arc::new(container),
    )
}

fn request(method: Method, path: &str, body: Option<Value>, container: &Arc<Container>) -> Request {
    let mut headers = HeaderMap::new();
    let bytes = body.map_or_else(Bytes::new, |value| {
        headers.insert("content-type", "application/json".parse().unwrap());
        Bytes::from(serde_json::to_vec(&value).expect("serialize request"))
    });
    Request::new(
        method,
        path.parse::<Uri>().expect("valid URI"),
        headers,
        quench_http::body::InboundBody::from_bytes(bytes),
        container.clone(),
    )
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

#[tokio::test]
async fn list_endpoints_refuse_ephemeral_database_storage() {
    let (app, container) = app().await;
    for path in ["/api/v1/tasks", "/api/v1/projects", "/api/v1/labels"] {
        let response = app.call(request(Method::GET, path, None, &container)).await;
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE, "{path}");
        assert!(
            body(response).await["error"]
                .as_str()
                .unwrap()
                .contains("Postgres")
        );
    }
}

#[tokio::test]
async fn empty_task_and_project_names_are_rejected_before_database_access() {
    let (app, container) = app().await;
    let cases = [
        (Method::POST, "/api/v1/projects", json!({"name":"  "})),
        (Method::POST, "/api/v1/labels", json!({"name":"  "})),
        (Method::PUT, "/api/v1/labels/label-id", json!({"name":""})),
        (
            Method::POST,
            "/api/v1/labels",
            json!({"name":"Errand","color":"green"}),
        ),
        (Method::POST, "/api/v1/tasks", json!({"content":"\t"})),
        (Method::PUT, "/api/v1/tasks/task-id", json!({"content":" "})),
        (
            Method::PUT,
            "/api/v1/projects/project-id",
            json!({"name":""}),
        ),
    ];

    for (method, path, payload) in cases {
        let response = app
            .call(request(method.clone(), path, Some(payload), &container))
            .await;
        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "{method} {path}"
        );
    }
}

#[tokio::test]
async fn syntactically_valid_writes_reach_the_postgres_requirement() {
    let (app, container) = app().await;
    let cases = [
        (Method::POST, "/api/v1/projects", json!({"name":"Work"})),
        (
            Method::POST,
            "/api/v1/labels",
            json!({"name":"Errand","color":"#00ff88"}),
        ),
        (
            Method::POST,
            "/api/v1/tasks",
            json!({"content":"Write tests"}),
        ),
        (
            Method::PUT,
            "/api/v1/tasks/task-id",
            json!({"content":"Write tests","is_completed":true}),
        ),
    ];

    for (method, path, payload) in cases {
        let response = app
            .call(request(method.clone(), path, Some(payload), &container))
            .await;
        assert_eq!(
            response.status(),
            StatusCode::SERVICE_UNAVAILABLE,
            "{method} {path}"
        );
    }
}

fn claims_with(scope: &str) -> Claims {
    Claims::for_audiences(
        "alice".to_string(),
        vec!["daybook".to_string()],
        scope.to_string(),
        None,
        3600,
    )
}

fn enforcing() -> JwtConfig {
    let mut config = JwtConfig::for_tests();
    config.auth_enabled = true;
    config
}

#[tokio::test]
async fn identity_extraction_uses_the_gatehouse_subject() {
    use daybook_service::routers::{OptionalClaims, actor};
    use quench_http::prelude::FromRequest;

    let (_app, container) = app().await;
    let mut request = request(Method::GET, "/", None, &container);
    request.extensions_mut().insert(claims_with("daybook:read"));
    let OptionalClaims(claims) = OptionalClaims::from_request(&mut request)
        .await
        .expect("identity extractor");
    assert_eq!(actor(claims.as_ref()), "alice");
}

#[test]
fn read_grant_does_not_allow_writes() {
    use daybook_service::routers::authz::can;

    let claims = claims_with("daybook:read");
    assert!(can(Some(&claims), &enforcing(), "read"));
    assert!(!can(Some(&claims), &enforcing(), "write"));
}

#[test]
fn write_grant_allows_writes() {
    use daybook_service::routers::authz::can;

    let claims = claims_with("daybook:write");
    assert!(can(Some(&claims), &enforcing(), "write"));
}

#[test]
fn no_claims_means_no_access_unless_auth_is_off() {
    use daybook_service::routers::authz::can;

    assert!(!can(None, &enforcing(), "read"));
    assert!(can(None, &JwtConfig::for_tests(), "write"));
}

#[tokio::test]
async fn a_read_only_token_cannot_write() {
    let (app, container) = enforcing_app().await;
    let mut write = request(
        Method::POST,
        "/api/v1/tasks",
        Some(json!({"content":"x"})),
        &container,
    );
    write.extensions_mut().insert(claims_with("daybook:read"));
    assert_eq!(app.call(write).await.status(), StatusCode::FORBIDDEN);

    let mut read = request(Method::GET, "/api/v1/tasks", None, &container);
    read.extensions_mut().insert(claims_with("daybook:write"));
    assert_eq!(app.call(read).await.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn invalid_repeat_rules_are_rejected_before_database_access() {
    let (app, container) = app().await;
    let due = "2090-03-02T09:00:00Z";
    let cases = [
        (
            "no due date",
            json!({"content":"x","recurrence":{"unit":"day","interval":1}}),
        ),
        (
            "zero interval",
            json!({"content":"x","due_at":due,"recurrence":{"unit":"day","interval":0}}),
        ),
        (
            "unknown zone",
            json!({"content":"x","due_at":due,"recurrence":{"unit":"day","interval":1,"timezone":"Mars/Olympus"}}),
        ),
        (
            "weekdays on a daily rule",
            json!({"content":"x","due_at":due,"recurrence":{"unit":"day","interval":1,"weekdays":[1]}}),
        ),
        (
            "weekday out of range",
            json!({"content":"x","due_at":due,"recurrence":{"unit":"week","interval":1,"weekdays":[8]}}),
        ),
    ];
    for (name, payload) in cases {
        for (method, path) in [
            (Method::POST, "/api/v1/tasks"),
            (Method::PUT, "/api/v1/tasks/task-id"),
        ] {
            let response = app
                .call(request(
                    method.clone(),
                    path,
                    Some(payload.clone()),
                    &container,
                ))
                .await;
            assert_eq!(
                response.status(),
                StatusCode::BAD_REQUEST,
                "{name}: {method} {path}"
            );
        }
    }

    // An unknown unit is not even a valid shape.
    let response = app
        .call(request(
            Method::POST,
            "/api/v1/tasks",
            Some(
                json!({"content":"x","due_at":due,"recurrence":{"unit":"fortnight","interval":1}}),
            ),
            &container,
        ))
        .await;
    assert!(response.status().is_client_error());
}

#[tokio::test]
async fn a_valid_repeat_rule_reaches_the_postgres_requirement() {
    let (app, container) = app().await;
    let response = app
        .call(request(
            Method::POST,
            "/api/v1/tasks",
            Some(json!({
                "content":"Water plants",
                "due_at":"2090-03-02T09:00:00Z",
                "recurrence":{"unit":"week","interval":2,"weekdays":[1,4],"timezone":"Europe/Warsaw"}
            })),
            &container,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}
