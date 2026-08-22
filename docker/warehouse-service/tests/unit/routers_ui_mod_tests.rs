use actix_web::{App, http::StatusCode, http::header, test, web};
use quench_auth::prelude::JwtConfig;
use warehouse_service::routers::ui::{
    crates_root, crates_root_slash, docker_root, docker_root_slash, root, root_slash,
};

fn config(auth_enabled: bool) -> JwtConfig {
    let mut config = JwtConfig::for_tests();
    config.auth_enabled = auth_enabled;
    config
}

macro_rules! redirect_test {
    ($test_name:ident, $service:expr, $uri:expr, $unauthenticated_location:expr) => {
        #[actix_web::test]
        async fn $test_name() {
            let app = test::init_service(
                App::new()
                    .app_data(web::Data::new(config(true)))
                    .service($service),
            )
            .await;
            let req = test::TestRequest::get().uri($uri).to_request();
            let resp = test::call_service(&app, req).await;
            assert!(resp.status().is_redirection());
            let location = resp
                .headers()
                .get(header::LOCATION)
                .unwrap()
                .to_str()
                .unwrap();
            assert!(location.contains($unauthenticated_location), "{location}");
        }
    };
}

/// `root`'s own route path is `#[get("")]` - only meaningful nested
/// inside a scope, the way production mounts it under `/ui`. Testing it
/// standalone with a bare `""`/`"/"` request path doesn't route the same
/// way, so this wraps it in the same `/ui` scope and hits `/ui` for real.
#[actix_web::test]
async fn root_redirects_to_login_when_unauthenticated() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config(true)))
            .service(web::scope("/ui").service(root)),
    )
    .await;
    let req = test::TestRequest::get().uri("/ui").to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_redirection());
    let location = resp
        .headers()
        .get(header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(location.contains("login"), "{location}");
}
redirect_test!(
    root_slash_redirects_to_login_when_unauthenticated,
    root_slash,
    "/",
    "login"
);
redirect_test!(
    docker_root_redirects_to_login_when_unauthenticated,
    docker_root,
    "/docker",
    "login"
);
redirect_test!(
    docker_root_slash_redirects_to_login_when_unauthenticated,
    docker_root_slash,
    "/docker/",
    "login"
);
redirect_test!(
    crates_root_redirects_to_login_when_unauthenticated,
    crates_root,
    "/crates",
    "login"
);
redirect_test!(
    crates_root_slash_redirects_to_login_when_unauthenticated,
    crates_root_slash,
    "/crates/",
    "login"
);

#[actix_web::test]
async fn root_redirects_home_when_authenticated() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config(false)))
            .service(web::scope("/ui").service(root)),
    )
    .await;
    let req = test::TestRequest::get().uri("/ui").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    let location = resp
        .headers()
        .get(header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(location.contains("/ui/home"), "{location}");
}

#[actix_web::test]
async fn docker_root_redirects_to_the_docker_catalog_when_authenticated() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config(false)))
            .service(docker_root),
    )
    .await;
    let req = test::TestRequest::get().uri("/docker").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::PERMANENT_REDIRECT);
    let location = resp
        .headers()
        .get(header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(location.contains("/ui/docker/catalog"), "{location}");
}

#[actix_web::test]
async fn crates_root_redirects_to_the_crates_catalog_when_authenticated() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config(false)))
            .service(crates_root),
    )
    .await;
    let req = test::TestRequest::get().uri("/crates").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::PERMANENT_REDIRECT);
    let location = resp
        .headers()
        .get(header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(location.contains("/ui/crates/catalog"), "{location}");
}
