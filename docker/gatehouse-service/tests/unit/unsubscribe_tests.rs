use gatehouse_service::PublicBase;
use gatehouse_service::notify::catalog;
use gatehouse_service::notify::prefs::Preferences;
use gatehouse_service::notify::unsubscribe::{Unsubscribe, issue_link};
use gatehouse_service::tokens::{PURPOSE_UNSUBSCRIBE, VerificationTokens};
use gatehouse_service::ui::pages::unsubscribe::{TokenQuery, unsubscribe_page, unsubscribe_submit};
use http::StatusCode;
use http_body_util::BodyExt;
use quench_db::prelude::Db;
use quench_http::prelude::{Form, Inject, Query};
use std::collections::HashMap;
use std::sync::Arc;

async fn db() -> Db {
    Db::connect("").await.expect("in-memory db")
}

fn failed() -> &'static catalog::Template {
    catalog::find("conveyor.run.failed").unwrap()
}

async fn body(resp: quench_http::response::Response) -> String {
    let bytes = resp
        .into_hyper()
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes();
    String::from_utf8(bytes.to_vec()).unwrap()
}

async fn link_token(tokens: &VerificationTokens, username: &str, template: &str) -> String {
    let base = PublicBase::resolve("https://mail.example.test", "");
    let link = issue_link(tokens, &base, username, template).await.unwrap();
    link.split("token=").nth(1).unwrap().to_string()
}

async fn open(tokens: &Arc<VerificationTokens>, token: &str) -> quench_http::response::Response {
    unsubscribe_page(
        Query(TokenQuery {
            token: Some(token.to_string()),
        }),
        Inject(tokens.clone()),
    )
    .await
}

async fn press(
    db: &Db,
    tokens: &Arc<VerificationTokens>,
    query_token: Option<&str>,
    form: &[(&str, &str)],
) -> quench_http::response::Response {
    unsubscribe_submit(
        Query(TokenQuery {
            token: query_token.map(str::to_string),
        }),
        Form(
            form.iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect::<HashMap<_, _>>(),
        ),
        Inject(Arc::new(db.clone())),
        Inject(tokens.clone()),
    )
    .await
}

#[tokio::test]
async fn opening_the_link_asks_and_changes_nothing() {
    let db = db().await;
    let tokens = Arc::new(VerificationTokens::in_memory());
    let token = link_token(&tokens, "alice", "conveyor.run.failed").await;

    let resp = open(&tokens, &token).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let html = body(resp).await;
    assert!(html.contains("ui_unsubscribe_submit"), "{html}");
    assert!(
        html.contains(&failed().label_key()),
        "names the kind: {html}"
    );
    assert!(
        html.contains(&format!("/unsubscribe?token={token}")),
        "the button posts with the token: {html}"
    );

    // Nothing happened: a mail scanner opening the link unsubscribes nobody.
    assert!(
        Preferences::new(&db)
            .is_subscribed("alice", failed())
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn pressing_the_button_turns_that_kind_off_for_that_person() {
    let db = db().await;
    let tokens = Arc::new(VerificationTokens::in_memory());
    let token = link_token(&tokens, "alice", "conveyor.run.failed").await;

    let resp = press(&db, &tokens, Some(&token), &[("unsubscribe", "1")]).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(body(resp).await.contains("ui_unsubscribe_done"));
    let prefs = Preferences::new(&db);
    assert!(!prefs.is_subscribed("alice", failed()).await.unwrap());
    assert!(
        prefs.is_subscribed("bob", failed()).await.unwrap(),
        "nobody else"
    );
    assert!(
        !prefs
            .is_subscribed("alice", catalog::find("conveyor.run.succeeded").unwrap())
            .await
            .unwrap(),
        "the other kind keeps its own default (off)"
    );
}

#[tokio::test]
async fn a_mail_providers_one_click_post_works() {
    // RFC 8058: POST to the exact URL from the header, body `List-Unsubscribe=One-Click`.
    let db = db().await;
    let tokens = Arc::new(VerificationTokens::in_memory());
    let token = link_token(&tokens, "alice", "conveyor.run.failed").await;
    let resp = press(
        &db,
        &tokens,
        Some(&token),
        &[("List-Unsubscribe", "One-Click")],
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(
        !Preferences::new(&db)
            .is_subscribed("alice", failed())
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn the_token_may_also_arrive_in_the_form() {
    let db = db().await;
    let tokens = Arc::new(VerificationTokens::in_memory());
    let token = link_token(&tokens, "alice", "conveyor.run.failed").await;
    let resp = press(&db, &tokens, None, &[("token", &token)]).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(
        !Preferences::new(&db)
            .is_subscribed("alice", failed())
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn doing_it_again_is_harmless_the_link_keeps_working() {
    let db = db().await;
    let tokens = Arc::new(VerificationTokens::in_memory());
    let token = link_token(&tokens, "alice", "conveyor.run.failed").await;
    for _ in 0..3 {
        let resp = press(&db, &tokens, Some(&token), &[("unsubscribe", "1")]).await;
        assert_eq!(resp.status(), StatusCode::OK);
    }
    assert_eq!(
        Preferences::new(&db)
            .overrides("alice")
            .await
            .unwrap()
            .len(),
        1
    );
    // ...and opening it afterwards still shows the page rather than "invalid".
    assert_eq!(open(&tokens, &token).await.status(), StatusCode::OK);
}

#[tokio::test]
async fn unknown_foreign_and_stale_tokens_are_refused_and_change_nothing() {
    let db = db().await;
    let tokens = Arc::new(VerificationTokens::in_memory());
    let other_purpose = tokens
        .issue(gatehouse_service::tokens::PURPOSE_VERIFY_EMAIL, "alice", 60)
        .await
        .unwrap();
    let not_a_ticket = tokens
        .issue(PURPOSE_UNSUBSCRIBE, "alice", 60)
        .await
        .unwrap();
    let gone_kind = tokens
        .issue(
            PURPOSE_UNSUBSCRIBE,
            &Unsubscribe {
                username: "alice".into(),
                template: "conveyor.run.retired".into(),
            }
            .encode(),
            60,
        )
        .await
        .unwrap();

    for token in ["nonsense", "", &other_purpose, &not_a_ticket, &gone_kind] {
        let page = open(&tokens, token).await;
        assert_eq!(page.status(), StatusCode::NOT_FOUND, "{token:?}");
        assert!(body(page).await.contains("ui_unsubscribe_invalid"));

        let done = press(&db, &tokens, Some(token), &[("unsubscribe", "1")]).await;
        assert_eq!(done.status(), StatusCode::NOT_FOUND, "{token:?}");
    }
    let none = press(&db, &tokens, None, &[]).await;
    assert_eq!(none.status(), StatusCode::NOT_FOUND);
    assert!(
        Preferences::new(&db)
            .overrides("alice")
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn one_persons_link_never_touches_someone_elses_choices() {
    let db = db().await;
    let tokens = Arc::new(VerificationTokens::in_memory());
    let bobs = link_token(&tokens, "bob", "conveyor.run.failed").await;
    press(&db, &tokens, Some(&bobs), &[("unsubscribe", "1")]).await;
    let prefs = Preferences::new(&db);
    assert!(!prefs.is_subscribed("bob", failed()).await.unwrap());
    assert!(prefs.is_subscribed("alice", failed()).await.unwrap());
}
