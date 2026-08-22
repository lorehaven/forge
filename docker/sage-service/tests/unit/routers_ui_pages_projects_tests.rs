//! Unit tests for `routers/ui/pages/projects.rs`.

use actix_web::http::StatusCode;
use actix_web::web::Data;
use actix_web::{App, test};
use quench_auth::prelude::JwtConfig;
use quench_db::InMemoryDb;
use quench_db::prelude::{Crud, Db};
use sage_service::domain::models::Project;
use sage_service::routers::ui::pages::projects::scope;

#[actix_web::test]
async fn new_modal_renders_the_create_form() {
    let app = test::init_service(App::new().service(scope())).await;
    let req = test::TestRequest::get()
        .uri("/projects/new-modal")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body = test::read_body(resp).await;
    let html = String::from_utf8(body.to_vec()).expect("utf8 html");
    assert!(html.contains("new-project-modal"));
    assert!(html.contains("project-name"));
}

#[actix_web::test]
async fn create_project_is_unauthorized_without_a_claim_when_auth_is_required() {
    let mut config = JwtConfig::for_tests();
    config.auth_enabled = true;
    let app = test::init_service(
        App::new()
            .app_data(Data::new(config))
            .app_data(Data::new(Db::InMemory(InMemoryDb::new())))
            .service(scope()),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/projects/create")
        .set_form([("name", "My Project")])
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn create_project_stores_the_project_owned_by_the_admin_bypass_user() {
    let db = Db::InMemory(InMemoryDb::new());
    let app = test::init_service(
        App::new()
            .app_data(Data::new(JwtConfig::for_tests()))
            .app_data(Data::new(db.clone()))
            .service(scope()),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/projects/create")
        .set_form([("name", "My Project")])
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let redirect = resp
        .headers()
        .get("HX-Redirect")
        .expect("HX-Redirect header")
        .to_str()
        .unwrap()
        .to_string();
    assert!(redirect.contains("/ui/home?project_id="));

    let projects = db.repository::<Project>().list().await.unwrap();
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0].name, "My Project");
    assert_eq!(projects[0].owner, "admin");
}
