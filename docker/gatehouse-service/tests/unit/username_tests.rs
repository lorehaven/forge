use gatehouse_service::PublicBase;
use gatehouse_service::RateLimiter;
use gatehouse_service::email::Sender;
use gatehouse_service::realm::{self, RealmError, valid_username};
use gatehouse_service::test_support::RecordingSender;
use gatehouse_service::tokens::VerificationTokens;
use http::{HeaderMap, Method, StatusCode, Uri};
use quench_auth::domain::auth::{Permissions, Role, User};
use quench_db::prelude::{Crud, Db};
use quench_http::di::ContainerBuilder;
use quench_http::endpoint::Endpoint;
use quench_http::prelude::Request;
use std::sync::Arc;

async fn db() -> Db {
    Db::connect("").await.expect("in-memory db")
}

fn catalog() -> gatehouse_service::catalog::PermissionCatalog {
    let dir = std::env::temp_dir().join(format!("username-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("permissions.toml");
    std::fs::write(&path, "[services.gatehouse]\nactions = [\"read-users\"]\n").unwrap();
    let result =
        gatehouse_service::catalog::PermissionCatalog::load_from(&path.to_string_lossy()).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

const HOSTILE: [&str; 12] = [
    "<img src=x onerror=alert(1)>",
    "<script>alert(1)</script>",
    "\"><svg onload=alert(1)>",
    "a b",
    "tab\tname",
    "new\nline",
    "quo\"te",
    "sin'gle",
    "slash/name",
    "back\\slash",
    "zoë",
    "名前",
];

#[test]
fn ordinary_usernames_are_accepted() {
    for name in [
        "alice",
        "Alice",
        "a",
        "user01",
        "first.last",
        "first_last",
        "first-last",
        "alice@example.com",
        "alice+tag",
        "e2e-mail-test",
        "bdd-registrant",
        "losseheil",
    ] {
        assert!(valid_username(name), "{name}");
    }
    assert!(valid_username(&"a".repeat(64)));
}

#[test]
fn anything_that_could_be_markup_or_a_path_is_refused() {
    for name in HOSTILE {
        assert!(!valid_username(name), "{name:?}");
    }
    assert!(!valid_username(""));
    assert!(!valid_username(&"a".repeat(65)));
    assert!(!valid_username("<"));
    assert!(!valid_username("&amp;"));
    assert!(!valid_username("%3Cscript%3E"));
}

#[tokio::test]
async fn creating_an_account_with_such_a_name_is_refused_and_creates_nothing() {
    let db = db().await;
    for name in HOSTILE {
        let err = realm::create(
            &db,
            &catalog(),
            true,
            name,
            "password",
            vec![Role::User],
            Permissions::new(),
            None,
        )
        .await
        .unwrap_err();
        assert!(
            matches!(err, RealmError::UsernameInvalid),
            "{name:?}: {err:?}"
        );
    }
    assert!(
        realm::list(&db).await.unwrap().is_empty(),
        "nothing was created"
    );
}

#[tokio::test]
async fn the_invited_path_applies_the_same_rule() {
    let db = db().await;
    let err = realm::create_invited(
        &db,
        &catalog(),
        true,
        "<b>x</b>",
        vec![Role::User],
        Permissions::new(),
        "x@example.test",
    )
    .await
    .unwrap_err();
    assert!(matches!(err, RealmError::UsernameInvalid));
}

#[tokio::test]
async fn surrounding_space_is_still_trimmed_before_the_check() {
    let db = db().await;
    let user = realm::create(
        &db,
        &catalog(),
        true,
        "  alice  ",
        "password",
        vec![Role::User],
        Permissions::new(),
        None,
    )
    .await
    .unwrap();
    assert_eq!(user.username, "alice");
}

#[test]
fn the_error_reports_itself_properly() {
    let err = RealmError::UsernameInvalid;
    assert_eq!(err.status(), StatusCode::BAD_REQUEST);
    assert_eq!(err.i18n_key(), "ui_admin_error_username_invalid");
    assert!(err.message().contains("64"));
}

async fn register_app() -> (Arc<dyn Endpoint>, Arc<quench_http::di::Container>, Db) {
    gatehouse_service::ui::pages::register::register_routes();
    let db = db().await;
    let recorder = Arc::new(RecordingSender::default());
    let sender: Arc<dyn Sender> = recorder;
    let container = ContainerBuilder::new()
        .provide(catalog())
        .provide(db.clone())
        .provide(sender)
        .provide(PublicBase::resolve("https://mail.example.test", ""))
        .provide(RateLimiter::in_memory())
        .provide_arc(Arc::new(VerificationTokens::in_memory()))
        .build()
        .await
        .unwrap();
    (
        quench_starter::http::discover_and_mount("/"),
        Arc::new(container),
        db,
    )
}

#[tokio::test]
async fn public_registration_refuses_a_markup_username() {
    let (app, container, db) = register_app().await;
    for name in ["<img src=x onerror=alert(1)>", "a b", "zoë"] {
        let encoded = serde_urlencoded::to_string([
            ("username", name),
            ("password", "correct-horse"),
            ("email", "alice@example.com"),
        ])
        .unwrap();
        let resp = app
            .call(Request::new(
                Method::POST,
                "/ui/register".parse::<Uri>().unwrap(),
                HeaderMap::new(),
                quench_http::body::InboundBody::from_bytes(bytes::Bytes::from(encoded)),
                container.clone(),
            ))
            .await;
        assert_eq!(resp.status(), StatusCode::FOUND);
        let target = resp
            .into_hyper()
            .headers()
            .get("location")
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        assert!(
            target.contains("err=ui_admin_error_username_invalid"),
            "{name:?}: {target}"
        );
    }
    assert!(realm::list(&db).await.unwrap().is_empty());
}

#[test]
fn the_register_page_can_show_the_username_error() {
    assert_eq!(
        gatehouse_service::ui::pages::register::known_error_key("ui_admin_error_username_invalid"),
        Some("ui_admin_error_username_invalid")
    );
}

/// Old data, from before the rule: a hostile username already in the database.
/// The admin pages must show it as text. That is the job of the page renderer
/// (`quench-web`), which only escapes text from 0.1.13 on - so this stays ignored
/// until forge depends on that release (or a local `[patch]` points at the fix).
#[tokio::test]
async fn a_hostile_legacy_username_is_shown_as_text_on_the_admin_list() {
    use gatehouse_service::test_support::auth_disabled_guard;
    use quench_auth::domain::jwt::JwtConfig;
    let _guard = auth_disabled_guard().await;
    gatehouse_service::ui::pages::admin::register_routes();
    let db = db().await;
    // Straight into the table, as it could have been before the rule existed.
    db.repository::<User>()
        .create(
            &User::new(
                "<img src=x onerror=alert(document.cookie)>".to_string(),
                "pw".to_string(),
                vec![Role::User],
                Permissions::new(),
                None,
            )
            .unwrap(),
        )
        .await
        .expect("plant legacy user");
    realm::create(
        &db,
        &catalog(),
        true,
        "admin",
        "pw",
        vec![Role::Admin],
        Permissions::new(),
        None,
    )
    .await
    .unwrap();
    let container = ContainerBuilder::new()
        .provide(JwtConfig::for_tests())
        .provide(catalog())
        .provide(db)
        .provide_arc(quench_auth::domain::session::SessionDb::init(
            quench_cache::CacheStore::in_memory(),
        ))
        .build()
        .await
        .unwrap();
    let resp = quench_starter::http::discover_and_mount("/")
        .call(Request::new(
            Method::GET,
            "/ui/admin/users".parse::<Uri>().unwrap(),
            HeaderMap::new(),
            quench_http::body::InboundBody::from_bytes(bytes::Bytes::new()),
            Arc::new(container),
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    use http_body_util::BodyExt;
    let bytes = resp
        .into_hyper()
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes();
    let html = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(!html.contains("<img src=x"), "raw markup reached the page");
    assert!(html.contains("&lt;img src=x onerror=alert(document.cookie)&gt;"));
}
