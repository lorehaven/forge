//! HTTP-level test for `routers/ui/pages/pipelines.rs`'s `runs_list_page`.
//! Everything in this handler past the auth check does real database
//! queries (`projects::list_all`, `repos::list`, `queue::count_runs`) - out
//! of scope here (the API/scheduler side of this crate's coverage push owns
//! that). What's reachable without a real database is the auth-redirect
//! branch, since `Db::connect("")`'s in-memory backend still needs to be
//! registered as `web::Data` for the handler's extractors to succeed at all.

use actix_web::http::StatusCode;
use actix_web::web::Data;
use actix_web::{App, test as actix_test};
use conveyor_service::config::ConveyorConfig;
use conveyor_service::routers::ui;
use quench_auth::prelude::JwtConfig;
use quench_db::prelude::Db;

#[actix_web::test]
async fn runs_list_page_redirects_to_login_when_auth_is_enabled_and_there_is_no_session() {
    let mut config = JwtConfig::for_tests();
    config.auth_enabled = true;
    let db = Db::connect("").await.expect("in-memory database");

    let app = actix_test::init_service(
        App::new()
            .app_data(Data::new(config.clone()))
            .app_data(Data::new(ConveyorConfig::default()))
            .app_data(Data::new(db))
            .service(ui::scope(config)),
    )
    .await;

    let req = actix_test::TestRequest::get().uri("/ui/runs").to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FOUND);
}
