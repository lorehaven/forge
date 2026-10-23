use gatehouse_service::bootstrap::seed_users;
use quench_auth::prelude::{Role, User};
use quench_db::prelude::{Crud, Db};

const KEYS: &[&str] = &[
    "AUTH_BOOTSTRAP",
    "SERVICE_USERNAME",
    "SERVICE_PASSWORD",
    "SERVICE_TECH_USERNAME",
    "SERVICE_TECH_PASSWORD",
];

fn env_lock() -> &'static std::sync::Mutex<()> {
    static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(()))
}

struct EnvGuard(#[allow(dead_code)] std::sync::MutexGuard<'static, ()>);

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for key in KEYS {
            quench_core::env::remove(key);
        }
    }
}

fn clean_env() -> EnvGuard {
    let guard = env_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    for key in KEYS {
        quench_core::env::remove(key);
    }
    EnvGuard(guard)
}

#[tokio::test]
async fn seed_users_does_nothing_when_bootstrap_is_off() {
    let _guard = clean_env();
    let db = Db::connect("").await.expect("in-memory db");

    seed_users(&db).await;

    let repo = db.repository::<User>();
    assert!(repo.read("admin").await.unwrap_or(None).is_none());
}

#[tokio::test]
async fn seed_users_creates_the_admin_account_when_enabled() {
    let _guard = clean_env();
    quench_core::env::set("AUTH_BOOTSTRAP", "true");
    quench_core::env::set("SERVICE_USERNAME", "admin");
    quench_core::env::set("SERVICE_PASSWORD", "admin-password");

    let db = Db::connect("").await.expect("in-memory db");
    seed_users(&db).await;

    let repo = db.repository::<User>();
    let admin = repo.read("admin").await.expect("read").expect("present");
    let roles: Vec<Role> = quench_serde::from_value(&admin.roles).expect("roles");
    assert!(roles.contains(&Role::Admin));
}

#[tokio::test]
async fn seed_users_does_not_overwrite_an_existing_admin() {
    let _guard = clean_env();
    quench_core::env::set("AUTH_BOOTSTRAP", "true");
    quench_core::env::set("SERVICE_USERNAME", "admin");
    quench_core::env::set("SERVICE_PASSWORD", "first-password");

    let db = Db::connect("").await.expect("in-memory db");
    seed_users(&db).await;

    // A second boot with a different SERVICE_PASSWORD must not touch the
    // already-seeded account.
    quench_core::env::set("SERVICE_PASSWORD", "second-password");
    seed_users(&db).await;

    let repo = db.repository::<User>();
    let admin = repo.read("admin").await.expect("read").expect("present");
    assert!(admin.verify_password("first-password"));
    assert!(!admin.verify_password("second-password"));
}

#[tokio::test]
async fn seed_users_skips_the_tech_account_when_unset() {
    let _guard = clean_env();
    quench_core::env::set("AUTH_BOOTSTRAP", "true");
    quench_core::env::set("SERVICE_USERNAME", "admin");
    quench_core::env::set("SERVICE_PASSWORD", "admin-password");

    let db = Db::connect("").await.expect("in-memory db");
    seed_users(&db).await;

    let repo = db.repository::<User>();
    assert!(repo.read("tech").await.unwrap_or(None).is_none());
}

#[tokio::test]
async fn seed_users_creates_the_tech_account_when_configured() {
    let _guard = clean_env();
    quench_core::env::set("AUTH_BOOTSTRAP", "true");
    quench_core::env::set("SERVICE_USERNAME", "admin");
    quench_core::env::set("SERVICE_PASSWORD", "admin-password");
    quench_core::env::set("SERVICE_TECH_USERNAME", "tech");
    quench_core::env::set("SERVICE_TECH_PASSWORD", "tech-password");

    let db = Db::connect("").await.expect("in-memory db");
    seed_users(&db).await;

    let repo = db.repository::<User>();
    let tech = repo.read("tech").await.expect("read").expect("present");
    let roles: Vec<Role> = quench_serde::from_value(&tech.roles).expect("roles");
    assert!(roles.contains(&Role::Service));
}
