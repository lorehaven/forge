//! Unit tests for `actix/domain/session.rs`.

use quench_auth::actix::domain::session::*;
use quench_cache::CacheStore;
use std::sync::Arc;

fn sessions() -> Arc<SessionDb> {
    SessionDb::init(CacheStore::in_memory())
}

#[actix_web::test]
async fn refresh_tokens_rotate_and_old_tokens_stop_working() {
    let sessions = sessions();
    let (session, first) = sessions.create("user", 3600).await.unwrap();

    let (_, second) = sessions.rotate(&first, 3600).await.unwrap().unwrap();

    assert!(
        sessions.rotate(&first, 3600).await.unwrap().is_none(),
        "a consumed refresh token must not work twice"
    );
    assert!(sessions.rotate(&second, 3600).await.unwrap().is_some());
    assert!(sessions.is_active(&session.id, "user").await.unwrap());
}

#[actix_web::test]
async fn revoking_ends_the_session_for_its_owner_only() {
    let sessions = sessions();
    let (session, token) = sessions.create("alice", 3600).await.unwrap();

    assert!(!sessions.revoke(&session.id, "bob").await.unwrap());
    assert!(sessions.is_active(&session.id, "alice").await.unwrap());

    assert!(sessions.revoke(&session.id, "alice").await.unwrap());
    assert!(!sessions.is_active(&session.id, "alice").await.unwrap());
    assert!(
        sessions.rotate(&token, 3600).await.unwrap().is_none(),
        "a revoked session's refresh token is dead too"
    );
}

#[actix_web::test]
async fn a_session_belongs_to_one_user() {
    let sessions = sessions();
    let (session, _) = sessions.create("alice", 3600).await.unwrap();
    assert!(!sessions.is_active(&session.id, "bob").await.unwrap());
}

#[actix_web::test]
async fn logout_revokes_by_refresh_token() {
    let sessions = sessions();
    let (session, token) = sessions.create("alice", 3600).await.unwrap();

    assert!(sessions.revoke_by_refresh_token(&token).await.unwrap());
    assert!(!sessions.is_active(&session.id, "alice").await.unwrap());
    assert!(!sessions.revoke_by_refresh_token(&token).await.unwrap());
}
