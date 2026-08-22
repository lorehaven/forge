use chrono::{DateTime, Duration, Utc};
use gatehouse_service::codes::AuthorizationCodeRow;
use quench_db::prelude::Model;

fn row(consumed_at: Option<DateTime<Utc>>, expires_at: DateTime<Utc>) -> AuthorizationCodeRow {
    AuthorizationCodeRow {
        code_hash: "hash".to_string(),
        client_id: "client".to_string(),
        username: "alice".to_string(),
        redirect_uri: "https://example.test/callback".to_string(),
        scope: "openid".to_string(),
        pkce_challenge: "challenge".to_string(),
        created_at: Utc::now(),
        expires_at,
        consumed_at,
    }
}

#[test]
fn is_usable_when_unconsumed_and_not_yet_expired() {
    let now = Utc::now();
    assert!(row(None, now + Duration::minutes(5)).is_usable(now));
}

#[test]
fn is_not_usable_once_consumed() {
    let now = Utc::now();
    assert!(!row(Some(now), now + Duration::minutes(5)).is_usable(now));
}

#[test]
fn is_not_usable_once_expired() {
    let now = Utc::now();
    assert!(!row(None, now - Duration::seconds(1)).is_usable(now));
}

#[test]
fn is_not_usable_exactly_at_the_expiry_instant() {
    let now = Utc::now();
    assert!(!row(None, now).is_usable(now));
}

#[test]
fn table_name_is_schema_qualified() {
    assert!(AuthorizationCodeRow::table_name().ends_with(".authorization_codes"));
}

#[test]
fn columns_lists_every_field_including_the_primary_key() {
    let columns = AuthorizationCodeRow::columns();
    assert_eq!(columns.len(), 9);
    assert!(columns.contains(&"code_hash"));
    assert!(columns.contains(&"consumed_at"));
}

#[test]
fn primary_key_name_is_code_hash() {
    assert_eq!(AuthorizationCodeRow::primary_key_name(), "code_hash");
}
