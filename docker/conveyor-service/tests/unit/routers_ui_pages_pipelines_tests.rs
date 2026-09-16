//! HTTP-level test for `routers/ui/pages/pipelines.rs`'s `runs_list_page`.
//! Everything in this handler past the auth check does real database
//! queries (`projects::list_all`, `repos::list`, `queue::count_runs`) - out
//! of scope here (the API/scheduler side of this crate's coverage push owns
//! that). What's reachable without a real database is the auth-redirect
//! branch, since `Db::connect("")`'s in-memory backend still needs to be
//! registered in the DI container for the handler's extractors to succeed
//! at all.
//!
//! `tests/unit.rs` is a separate test binary from `tests/integration.rs`
//! (no shared `support` module), so this file builds its own minimal
//! `discover_and_mount` + hand-built `Request` helpers.

use bytes::Bytes;
use conveyor_service::config::ConveyorConfig;
use http::{HeaderMap, Method, StatusCode, Uri};
use quench_auth::domain::jwt::JwtConfig;
use quench_db::prelude::Db;
use quench_http::di::ContainerBuilder;
use quench_http::endpoint::Endpoint;
use quench_http::request::Request;
use std::sync::Arc;

#[tokio::test]
async fn runs_list_page_redirects_to_login_when_auth_is_enabled_and_there_is_no_session() {
    let mut config = JwtConfig::for_tests();
    config.auth_enabled = true;
    let db = Db::connect("").await.expect("in-memory database");

    conveyor_service::routers::ui::register_routes();
    let container = ContainerBuilder::new()
        .provide(config)
        .provide(ConveyorConfig::default())
        .provide(db)
        .build()
        .await
        .unwrap();
    let container = Arc::new(container);
    let app: Arc<dyn Endpoint> = quench_starter::http::discover_and_mount("/");

    let req = Request::new(
        Method::GET,
        "/ui/runs".parse::<Uri>().unwrap(),
        HeaderMap::new(),
        quench_http::body::InboundBody::from_bytes(Bytes::new()),
        container,
    );
    let resp = app.call(req).await;
    assert_eq!(resp.status(), StatusCode::FOUND);
}
