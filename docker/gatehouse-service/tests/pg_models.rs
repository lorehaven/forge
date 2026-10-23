//! The gatehouse tables through `quench-db`'s `Crud` against a real database. Skipped unless
//! `GATEHOUSE_TEST_DATABASE_URL` points at one with the gatehouse module installed
//! (`foundry-service apply -i gatehouse`).

use gatehouse_service::app_tokens::AppTokenRow;
use gatehouse_service::clients::ClientRow;
use gatehouse_service::codes::AuthorizationCodeRow;
use gatehouse_service::notify::prefs::PrefRow;
use quench_core::time::{Duration, Time};
use quench_db::prelude::{Crud, Database, Db};

#[tokio::test]
async fn the_gatehouse_rows_round_trip_through_postgres() {
    let Ok(url) = std::env::var("GATEHOUSE_TEST_DATABASE_URL") else {
        return;
    };
    let db = Db::connect(&url).await.expect("connect");
    let unique = quench_core::uuid::Uuid::new_v4().simple().to_string();
    let user = format!("u{unique}");
    db.execute(&format!(
        "INSERT INTO auth.users (username, password, roles) VALUES ('{user}', 'x', '[]'::jsonb)"
    ))
    .await
    .expect("seed a user");
    let now = Time::from_unix(Time::now().timestamp());

    let clients = db.repository::<ClientRow>();
    let client = ClientRow {
        client_id: format!("c{unique}"),
        secret_hash: "hash".into(),
        redirect_uris: vec![
            "https://a.example/cb".into(),
            "https://b.example/cb?x=1".into(),
        ],
        allowed_scopes: vec!["sage:read".into()],
        created_at: now,
    };
    let created = clients.create(&client).await.expect("client");
    assert_eq!(created.redirect_uris, client.redirect_uris);
    assert_eq!(
        clients
            .read(&client.client_id)
            .await
            .unwrap()
            .unwrap()
            .redirect_uris,
        client.redirect_uris
    );

    let tokens = db.repository::<AppTokenRow>();
    let mut token = AppTokenRow {
        token_hash: format!("t{unique}"),
        id: format!("id{unique}"),
        username: user.clone(),
        name: "ci".into(),
        scope: "warehouse:read".into(),
        created_at: now,
        expires_at: Some(now + Duration::days(30)),
        last_used_at: None,
        revoked_at: None,
    };
    tokens.create(&token).await.expect("token");
    token.last_used_at = Some(now);
    tokens.update(&token).await.expect("update");
    let read = tokens.read(&token.token_hash).await.unwrap().unwrap();
    assert_eq!(
        (read.last_used_at, read.expires_at),
        (Some(now), Some(now + Duration::days(30)))
    );

    let codes = db.repository::<AuthorizationCodeRow>();
    let code = AuthorizationCodeRow {
        code_hash: format!("code{unique}"),
        client_id: client.client_id.clone(),
        username: user.clone(),
        redirect_uri: "https://a.example/cb".into(),
        scope: "sage:read".into(),
        pkce_challenge: "challenge".into(),
        created_at: now,
        expires_at: now + Duration::minutes(1),
        consumed_at: None,
    };
    codes.create(&code).await.expect("code");
    assert_eq!(
        codes
            .read(&code.code_hash)
            .await
            .unwrap()
            .unwrap()
            .expires_at,
        now + Duration::minutes(1)
    );

    let prefs = db.repository::<PrefRow>();
    let pref = PrefRow {
        key: format!("{user}|welcome"),
        username: user.clone(),
        template: "welcome".into(),
        enabled: false,
        updated_at: now,
    };
    prefs.create(&pref).await.expect("pref");
    assert!(!prefs.read(&pref.key).await.unwrap().unwrap().enabled);

    prefs.delete(&pref.key).await.unwrap();
    codes.delete(&code.code_hash).await.unwrap();
    tokens.delete(&token.token_hash).await.unwrap();
    clients.delete(&client.client_id).await.unwrap();
    db.execute(&format!("DELETE FROM auth.users WHERE username = '{user}'"))
        .await
        .unwrap();
}
