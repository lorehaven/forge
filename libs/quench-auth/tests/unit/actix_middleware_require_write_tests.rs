//! Unit tests for `actix/middleware/require_write.rs`.

use actix_web::HttpMessage;
use actix_web::dev::{Service, Transform};
use actix_web::{http::StatusCode, test};
use quench_auth::actix::domain::jwt::{Claims, JwtConfig};
use quench_auth::actix::middleware::require_write::RequireWrite;

fn config() -> JwtConfig {
    envmnt::set("JWT_SECRET", "test_secret");
    let mut config = JwtConfig::init();
    config.service_name = "sage".to_string();
    config.auth_enabled = true;
    config
}

fn claims(scope: &str) -> Claims {
    Claims::for_audiences(
        "user".to_string(),
        vec!["sage".to_string()],
        scope.to_string(),
        None,
        900,
    )
}

/// Every request that reaches the inner service in these tests hits
/// `test::ok_service()`, which always answers 200 - so a 200 here means
/// `RequireWrite` let it through, and anything else means it did not.
async fn middleware(
    config: JwtConfig,
) -> impl Service<
    actix_web::dev::ServiceRequest,
    Response = actix_web::dev::ServiceResponse<impl actix_web::body::MessageBody>,
    Error = actix_web::Error,
> {
    RequireWrite::new(config)
        .new_transform(test::ok_service())
        .await
        .expect("transform never fails")
}

#[actix_web::test]
async fn reads_never_need_the_write_permission() {
    let mw = middleware(config()).await;
    for method in [
        actix_web::http::Method::GET,
        actix_web::http::Method::HEAD,
        actix_web::http::Method::OPTIONS,
    ] {
        let req = test::TestRequest::default()
            .method(method.clone())
            .uri("/x")
            .to_srv_request();
        let res = mw.call(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK, "{method} should pass through");
    }
}

#[actix_web::test]
async fn a_write_without_the_permission_is_refused() {
    let mw = middleware(config()).await;
    let req = test::TestRequest::post().uri("/x").to_srv_request();
    req.extensions_mut().insert(claims("user sage:read"));

    let res = mw.call(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

#[actix_web::test]
async fn a_write_grant_is_accepted() {
    let mw = middleware(config()).await;
    let req = test::TestRequest::post().uri("/x").to_srv_request();
    req.extensions_mut().insert(claims("user sage:write"));

    let res = mw.call(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

/// A grant on a different service must not satisfy this one.
#[actix_web::test]
async fn a_write_grant_on_another_service_does_not_count() {
    let mw = middleware(config()).await;
    let req = test::TestRequest::post().uri("/x").to_srv_request();
    req.extensions_mut().insert(claims("user warehouse:write"));

    let res = mw.call(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

#[actix_web::test]
async fn a_wildcard_role_needs_no_enumerated_grant() {
    let mw = middleware(config()).await;
    let req = test::TestRequest::delete().uri("/x").to_srv_request();
    req.extensions_mut().insert(claims("admin"));

    let res = mw.call(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

/// Simulates the middleware mounted without `Auth` ahead of it: no claims ever
/// land in extensions. This has to fail closed, since a mounting mistake should
/// end up denying access rather than granting it.
#[actix_web::test]
async fn missing_claims_is_refused_not_ignored() {
    let mw = middleware(config()).await;
    let req = test::TestRequest::post().uri("/x").to_srv_request();

    let res = mw.call(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

/// The realm-wide dev switch has to bypass this middleware exactly like it
/// bypasses `Auth` - otherwise turning auth off would turn write access off
/// instead of leaving it unchecked like everything else.
#[actix_web::test]
async fn auth_disabled_bypasses_the_check_entirely() {
    let mut disabled = config();
    disabled.auth_enabled = false;
    let mw = middleware(disabled).await;
    let req = test::TestRequest::post().uri("/x").to_srv_request();

    let res = mw.call(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}
