//! Seeding the realm's accounts.
//!
//! Gatehouse is the only service that writes users, so this lives here rather
//! than in `quench-auth` - a relying party has no business creating an account.

use quench_auth::prelude::{Permissions, Role, User};
use quench_db::prelude::{Crud, Db};

/// Creates the admin and machine-to-machine accounts if they are missing.
///
/// Existing users are never overwritten - once the realm has an account, its
/// password is whatever people last set, not what the environment says.
pub async fn seed_users(db: &Db) {
    if !envmnt::is_or("AUTH_BOOTSTRAP", false) {
        tracing::info!("AUTH_BOOTSTRAP is off, leaving the realm's users alone");
        return;
    }

    seed(
        db,
        &envmnt::get_or_panic("SERVICE_USERNAME"),
        &envmnt::get_or_panic("SERVICE_PASSWORD"),
        Role::Admin,
        "admin",
    )
    .await;

    // The machine-to-machine identity, e.g. sage calling switchboard. Optional:
    // a deployment with no service-to-service traffic does not need it.
    let tech_user = envmnt::get_or("SERVICE_TECH_USERNAME", "");
    let tech_password = envmnt::get_or("SERVICE_TECH_PASSWORD", "");
    if tech_user.is_empty() || tech_password.is_empty() {
        tracing::warn!(
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
        tracing::info!("{label} user {username} already exists");
        return;
    }

    // No permissions: both seeded accounts hold a wildcard role, which grants
    // everything without any of it being written down. Enumerating grants here
    // would go stale as soon as the estate gained a service.
    let Ok(user) = User::new(
        username.to_string(),
        password.to_string(),
        vec![role],
        Permissions::new(),
        None,
    ) else {
        tracing::error!("failed to hash the {label} password; {username} not created");
        return;
    };

    match repo.create(&user).await {
        Ok(_) => tracing::info!("created {label} user {username}"),
        Err(err) => tracing::error!("failed to create {label} user {username}: {err}"),
    }
}
