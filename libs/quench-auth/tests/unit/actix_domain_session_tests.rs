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

/// What makes removing someone's permissions take effect now rather than
/// whenever their access token happens to expire.
#[actix_web::test]
async fn revoking_everything_ends_every_session_one_user_holds() {
    let sessions = sessions();
    let (first, first_token) = sessions.create("alice", 3600).await.unwrap();
    let (second, _) = sessions.create("alice", 3600).await.unwrap();
    let (bystander, _) = sessions.create("bob", 3600).await.unwrap();

    assert_eq!(sessions.revoke_all("alice").await.unwrap(), 2);

    assert!(!sessions.is_active(&first.id, "alice").await.unwrap());
    assert!(!sessions.is_active(&second.id, "alice").await.unwrap());
    assert!(
        sessions.rotate(&first_token, 3600).await.unwrap().is_none(),
        "the refresh tokens have to die with the sessions"
    );
    assert!(
        sessions.is_active(&bystander.id, "bob").await.unwrap(),
        "one user's revocation must not touch another's"
    );

    assert_eq!(sessions.revoke_all("alice").await.unwrap(), 0);
    assert!(sessions.sessions_for("alice").await.unwrap().is_empty());
}

/// The index is what `revoke_all` walks. A session missing from it would survive
/// a revocation, so every path that creates or extends one has to maintain it.
#[actix_web::test]
async fn the_index_tracks_sessions_through_creation_rotation_and_logout() {
    let sessions = sessions();
    let (session, token) = sessions.create("alice", 3600).await.unwrap();

    assert_eq!(
        sessions.sessions_for("alice").await.unwrap(),
        vec![session.id.clone()]
    );

    // Rotation keeps the same session id, so the index should not grow.
    let (rotated, _) = sessions.rotate(&token, 3600).await.unwrap().unwrap();
    assert_eq!(rotated.id, session.id);
    assert_eq!(
        sessions.sessions_for("alice").await.unwrap(),
        vec![session.id.clone()]
    );

    sessions.revoke(&session.id, "alice").await.unwrap();
    assert!(sessions.sessions_for("alice").await.unwrap().is_empty());
}

#[actix_web::test]
async fn a_logout_clears_its_index_entry_too() {
    let sessions = sessions();
    let (_, token) = sessions.create("alice", 3600).await.unwrap();

    assert!(sessions.revoke_by_refresh_token(&token).await.unwrap());
    assert!(
        sessions.sessions_for("alice").await.unwrap().is_empty(),
        "a logged-out session left in the index would be reported as live"
    );
}
