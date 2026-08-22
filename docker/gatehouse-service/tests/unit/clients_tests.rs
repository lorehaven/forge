use chrono::Utc;
use gatehouse_service::clients::{ClientRow, hash_secret, redirect_base_url, seed_clients, upsert};
use quench_db::prelude::{Crud, Db};

fn env_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: std::sync::OnceLock<tokio::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

fn row(client_id: &str, secret: &str, redirect_uris: Vec<String>) -> ClientRow {
    ClientRow {
        client_id: client_id.to_string(),
        secret_hash: hash_secret(secret),
        redirect_uris,
        allowed_scopes: vec!["openid".to_string()],
        created_at: Utc::now(),
    }
}

#[test]
fn hash_secret_is_deterministic_and_not_the_plaintext() {
    let a = hash_secret("s3cret");
    let b = hash_secret("s3cret");
    assert_eq!(a, b);
    assert_ne!(a, "s3cret");
}

#[test]
fn hash_secret_differs_for_different_input() {
    assert_ne!(hash_secret("one"), hash_secret("two"));
}

#[test]
fn secret_matches_only_the_original_plaintext() {
    let row = row("client-a", "correct-secret", vec![]);
    assert!(row.secret_matches("correct-secret"));
    assert!(!row.secret_matches("wrong-secret"));
}

#[test]
fn redirect_uri_matches_only_a_registered_uri() {
    let row = row(
        "client-a",
        "secret",
        vec!["https://example.test/callback".to_string()],
    );
    assert!(row.redirect_uri_matches("https://example.test/callback"));
    assert!(!row.redirect_uri_matches("https://evil.test/callback"));
}

#[tokio::test]
async fn redirect_base_url_reads_ui_url_and_trims_the_home_suffix() {
    let _guard = env_lock().lock().await;
    unsafe { std::env::set_var("CONVEYOR_UI_URL", "https://conveyor.example.test/ui/home") };
    assert_eq!(
        redirect_base_url("conveyor"),
        Some("https://conveyor.example.test".to_string())
    );
    unsafe { std::env::remove_var("CONVEYOR_UI_URL") };
}

#[tokio::test]
async fn redirect_base_url_falls_back_to_the_plain_url_var() {
    let _guard = env_lock().lock().await;
    unsafe { std::env::remove_var("WORKBENCH_UI_URL") };
    unsafe { std::env::set_var("WORKBENCH_URL", "https://workbench.example.test/") };
    assert_eq!(
        redirect_base_url("workbench"),
        Some("https://workbench.example.test".to_string())
    );
    unsafe { std::env::remove_var("WORKBENCH_URL") };
}

#[tokio::test]
async fn redirect_base_url_is_none_when_neither_var_is_set() {
    let _guard = env_lock().lock().await;
    unsafe { std::env::remove_var("NOTHING_HERE_UI_URL") };
    unsafe { std::env::remove_var("NOTHING_HERE_URL") };
    assert_eq!(redirect_base_url("nothing-here"), None);
}

#[tokio::test]
async fn redirect_base_url_uppercases_and_converts_dashes_to_underscores() {
    let _guard = env_lock().lock().await;
    unsafe { std::env::set_var("MY_SERVICE_NAME_UI_URL", "https://my-service.example.test") };
    assert_eq!(
        redirect_base_url("my-service-name"),
        Some("https://my-service.example.test".to_string())
    );
    unsafe { std::env::remove_var("MY_SERVICE_NAME_UI_URL") };
}

#[tokio::test]
async fn upsert_creates_then_updates_the_same_client() {
    let db = Db::connect("").await.expect("in-memory db");
    let repo = db.repository::<ClientRow>();

    upsert(&repo, row("client-a", "first-secret", vec![]))
        .await
        .expect("create");
    let created = repo.read("client-a").await.expect("read").expect("present");
    assert_eq!(created.secret_hash, hash_secret("first-secret"));

    upsert(&repo, row("client-a", "second-secret", vec![]))
        .await
        .expect("update");
    let updated = repo.read("client-a").await.expect("read").expect("present");
    assert_eq!(updated.secret_hash, hash_secret("second-secret"));
}

#[tokio::test]
async fn seed_clients_skips_entries_whose_secret_env_is_unset() {
    let _guard = env_lock().lock().await;
    let dir = tempfile::tempdir().expect("tempdir");
    let config_path = dir.path().join("clients.toml");
    std::fs::write(
        &config_path,
        r#"
            [[client]]
            client_id = "no-secret-client"
            secret_env = "GATEHOUSE_TEST_UNSET_CLIENT_SECRET"
        "#,
    )
    .expect("write config");

    unsafe { std::env::set_var("CLIENTS_CONFIG", config_path.to_str().expect("utf8")) };
    unsafe { std::env::remove_var("GATEHOUSE_TEST_UNSET_CLIENT_SECRET") };

    let db = Db::connect("").await.expect("in-memory db");
    seed_clients(&db).await.expect("seed");

    let repo = db.repository::<ClientRow>();
    assert!(repo.read("no-secret-client").await.expect("read").is_none());

    unsafe { std::env::remove_var("CLIENTS_CONFIG") };
}

#[tokio::test]
async fn seed_clients_seeds_an_entry_with_a_configured_secret() {
    let _guard = env_lock().lock().await;
    let dir = tempfile::tempdir().expect("tempdir");
    let config_path = dir.path().join("clients.toml");
    std::fs::write(
        &config_path,
        r#"
            [[client]]
            client_id = "seeded-client"
            secret_env = "GATEHOUSE_TEST_SEEDED_CLIENT_SECRET"
            allowed_scopes = ["openid", "profile"]
        "#,
    )
    .expect("write config");

    unsafe { std::env::set_var("CLIENTS_CONFIG", config_path.to_str().expect("utf8")) };
    unsafe { std::env::set_var("GATEHOUSE_TEST_SEEDED_CLIENT_SECRET", "the-secret") };

    let db = Db::connect("").await.expect("in-memory db");
    seed_clients(&db).await.expect("seed");

    let repo = db.repository::<ClientRow>();
    let seeded = repo
        .read("seeded-client")
        .await
        .expect("read")
        .expect("present");
    assert!(seeded.secret_matches("the-secret"));
    assert_eq!(seeded.allowed_scopes, vec!["openid", "profile"]);

    unsafe { std::env::remove_var("CLIENTS_CONFIG") };
    unsafe { std::env::remove_var("GATEHOUSE_TEST_SEEDED_CLIENT_SECRET") };
}
