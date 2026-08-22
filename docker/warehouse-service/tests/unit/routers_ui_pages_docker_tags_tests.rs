use actix_web::{App, http::header, test, web};
use quench_auth::prelude::JwtConfig;
use warehouse_service::routers::ui::pages::docker::tags::docker_tags;

fn config(auth_enabled: bool) -> JwtConfig {
    let mut config = JwtConfig::for_tests();
    config.auth_enabled = auth_enabled;
    config
}

#[actix_web::test]
async fn redirects_to_login_when_auth_is_enabled_and_there_is_no_session() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config(true)))
            .service(docker_tags),
    )
    .await;
    let req = test::TestRequest::get()
        .uri("/docker/tags/my-repo")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_redirection());
}

#[actix_web::test]
async fn redirects_to_the_catalog_filtered_by_repository_when_authenticated() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config(false)))
            .service(docker_tags),
    )
    .await;
    let req = test::TestRequest::get()
        .uri("/docker/tags/my-repo")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        actix_web::http::StatusCode::PERMANENT_REDIRECT
    );
    let location = resp
        .headers()
        .get(header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(
        location.contains("/ui/docker/catalog?repo=my-repo"),
        "{location}"
    );
}
