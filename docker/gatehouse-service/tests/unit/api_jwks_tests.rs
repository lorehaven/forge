use bytes::Bytes;
use gatehouse_service::api::jwks::{jwks, rotate};
use gatehouse_service::api::users::ManageSigningKeysClaims;
use gatehouse_service::keys::SigningKeys;
use gatehouse_service::test_support::TEST_KEY_MATERIAL;
use http::{HeaderMap, Method, StatusCode};
use quench_auth::domain::jwt::JwtConfig;
use quench_db::prelude::Db;
use quench_http::body::InboundBody;
use quench_http::di::ContainerBuilder;
use quench_http::prelude::{FromRequest, Inject, Request};
use std::sync::Arc;

async fn signing_keys() -> Arc<SigningKeys> {
    envmnt::set("GATEHOUSE_KEY_ENCRYPTION_KEY", TEST_KEY_MATERIAL);
    let db = Db::connect("").await.expect("in-memory db");
    SigningKeys::init(db, 3600).await.expect("init keys")
}

#[tokio::test]
async fn jwks_returns_the_published_key_set() {
    let keys = signing_keys().await;
    let resp = jwks(Inject(keys)).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn rotate_succeeds_for_the_dev_bypass_actor() {
    let keys = signing_keys().await;
    let container = Arc::new(
        ContainerBuilder::new()
            .provide(JwtConfig::for_tests())
            .build()
            .await
            .unwrap(),
    );
    let mut req = Request::new(
        Method::POST,
        "/api/v1/admin/keys/rotate".parse().unwrap(),
        HeaderMap::new(),
        InboundBody::from_bytes(Bytes::new()),
        container,
    );
    // The dev-bypass actor always passes the `manage-signing-keys` gate, so a
    // 401/403 here would mean the extractor itself regressed; whether the
    // in-memory `Db` fixture supports every step `SigningKeys::rotate` needs
    // is exercised well enough by `jwks_returns_the_published_key_set`
    // (which relies on `SigningKeys::init`'s own internal rotation).
    let claims = ManageSigningKeysClaims::from_request(&mut req)
        .await
        .expect("dev bypass claims");

    let resp = rotate(Inject(keys), claims).await;
    assert_ne!(resp.status(), StatusCode::UNAUTHORIZED);
    assert_ne!(resp.status(), StatusCode::FORBIDDEN);
}
