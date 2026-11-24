use crate::support;
use quench_auth::domain::jwt::Claims;
use quench_db::{Db, InMemoryDb};
use quench_http::http::{Method, StatusCode};

async fn app(
    auth_enabled: bool,
) -> (
    std::sync::Arc<dyn quench_http::endpoint::Endpoint>,
    std::sync::Arc<quench_http::di::Container>,
) {
    gantry_service::routers::api::register_routes();
    let container = support::basic_container(
        support::jwt_config(auth_enabled),
        Db::InMemory(InMemoryDb::new()),
    )
    .await;
    support::app(container)
}

#[quench_test::test]
async fn info_names_the_service_and_reports_everything_allowed_when_auth_is_off() {
    let (app, container) = app(false).await;
    let resp = app
        .call(support::req(Method::GET, "/api/v1/info", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body = support::json_body(resp).await;
    assert_eq!(body["service"], "gantry");
    assert_eq!(body["actor"], "dev");
    assert_eq!(body["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(
        body["can"],
        quench_serde::json!(["read", "deploy", "scale", "activate", "rollback"])
    );
}

#[quench_test::test]
async fn info_reports_only_what_the_token_grants() {
    let (app, container) = app(true).await;
    let claims = Claims::for_audiences(
        "ci".to_string(),
        vec!["gantry".to_string()],
        "gantry:read gantry:scale".to_string(),
        None,
        3600,
    );
    let mut request = support::req(Method::GET, "/api/v1/info", &container);
    request.extensions_mut().insert(claims);

    let body = support::json_body(app.call(request).await).await;
    assert_eq!(body["actor"], "ci");
    assert_eq!(body["can"], quench_serde::json!(["read", "scale"]));
}

fn database_error(code: &str) -> gantry_service::domain::db::GantryError {
    gantry_service::domain::db::GantryError::Sql(quench_pg::Error::Server(Box::new(
        quench_pg::ServerError {
            code: code.to_string(),
            message: "invalid byte sequence for encoding UTF8: 0x00".to_string(),
            ..Default::default()
        },
    )))
}

#[quench_test::test]
async fn a_value_the_database_cannot_store_is_a_bad_request_and_says_nothing_of_the_database() {
    for code in ["22021", "22P02", "22001", "54000"] {
        let response =
            gantry_service::routers::api::ApiError::from(database_error(code)).into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{code}");
        assert!(
            !support::json_body(response)
                .await
                .to_string()
                .contains("UTF8"),
            "{code}"
        );
    }
}

#[quench_test::test]
async fn any_other_database_error_is_a_500_that_keeps_its_words_in_the_log() {
    let response =
        gantry_service::routers::api::ApiError::from(database_error("42P01")).into_response();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert!(
        !support::json_body(response)
            .await
            .to_string()
            .contains("UTF8")
    );
}
