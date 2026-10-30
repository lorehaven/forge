//! Seeding the realm's accounts - lives here since only gatehouse writes users.

use quench_auth::prelude::{Permissions, Role, User};
use quench_db::prelude::{Crud, Db};

/// Creates admin/machine accounts if missing; never overwrites an existing user.
pub async fn seed_users(db: &Db) {
    if !quench_core::env::is_or("AUTH_BOOTSTRAP", false) {
        quench_log::info!("AUTH_BOOTSTRAP is off, leaving the realm's users alone");
        return;
    }

    seed(
        db,
        &std::env::var("SERVICE_USERNAME").expect(concat!("SERVICE_USERNAME", " must be set")),
        &std::env::var("SERVICE_PASSWORD").expect(concat!("SERVICE_PASSWORD", " must be set")),
        Role::Admin,
        "admin",
    )
    .await;

    // Machine-to-machine identity (e.g. sage calling switchboard) - optional.
    let tech_user = quench_core::env::get_or("SERVICE_TECH_USERNAME", "");
    let tech_password = quench_core::env::get_or("SERVICE_TECH_PASSWORD", "");
    if tech_user.is_empty() || tech_password.is_empty() {
        quench_log::warn!(
            "SERVICE_TECH_USERNAME/SERVICE_TECH_PASSWORD not set: service-to-service \
             calls that use Basic auth will be rejected"
        );
        return;
    }
    seed(db, &tech_user, &tech_password, Role::Service, "service").await;
}

async fn seed(db: &Db, username: &str, password: &str, role: Role, label: &str) {
    let repo = db.repository::<User>();

    if repo.read(username).await.unwrap_or(None).is_some() {
        quench_log::info!("{label} user {username} already exists");
        return;
    }

    // No permissions - both seeded accounts hold a wildcard role instead.
    let Ok(user) = User::new(
        username.to_string(),
        password.to_string(),
        vec![role],
        Permissions::new(),
        None,
    ) else {
        quench_log::error!("failed to hash the {label} password; {username} not created");
        return;
    };

    match repo.create(&user).await {
        Ok(_) => quench_log::info!("created {label} user {username}"),
        Err(err) => quench_log::error!("failed to create {label} user {username}: {err}"),
    }
}
