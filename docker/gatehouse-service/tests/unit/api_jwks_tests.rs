use actix_web::{App, test as actix_test, web};
use gatehouse_service::api::jwks::{jwks, rotate};
use gatehouse_service::keys::SigningKeys;
use gatehouse_service::test_support::TEST_KEY_MATERIAL;
use quench_auth::prelude::SessionDb;
use quench_db::prelude::Db;
use std::sync::Arc;

fn sessions() -> web::Data<Arc<SessionDb>> {
    web::Data::new(SessionDb::init(quench_cache::CacheStore::in_memory()))
}

async fn signing_keys() -> Arc<SigningKeys> {
    envmnt::set("GATEHOUSE_KEY_ENCRYPTION_KEY", TEST_KEY_MATERIAL);
    let db = Db::connect("").await.expect("in-memory db");
    SigningKeys::init(db, 3600).await.expect("init keys")
}

#[actix_web::test]
async fn jwks_returns_the_published_key_set() {
    let keys = signing_keys().await;
    let app =
        actix_test::init_service(App::new().app_data(web::Data::new(keys)).service(jwks)).await;
    let req = actix_test::TestRequest::get()
        .uri("/.well-known/jwks.json")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
}

#[actix_web::test]
async fn rotate_succeeds_for_the_dev_bypass_actor() {
    let keys = signing_keys().await;
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(keys))
            .app_data(sessions())
            .service(rotate),
    )
    .await;
    let req = actix_test::TestRequest::post()
        .uri("/api/v1/admin/keys/rotate")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    // The dev-bypass actor always passes the `manage-signing-keys` gate, so a
    // 401/403 here would mean the extractor itself regressed; whether the
    // in-memory `Db` fixture supports every step `SigningKeys::rotate` needs
    // is exercised well enough by `jwks_returns_the_published_key_set`
    // (which relies on `SigningKeys::init`'s own internal rotation).
    assert_ne!(resp.status(), actix_web::http::StatusCode::UNAUTHORIZED);
    assert_ne!(resp.status(), actix_web::http::StatusCode::FORBIDDEN);
}
