//! The rules for changing the realm's users.
//!
//! Both callers live in this service - the JSON API under `api::users` and the
//! admin pages under `ui::pages::admin` - and both go through here. The rules
//! are the substance of the feature and the CRUD around them is mechanical, so
//! having two copies would mean the UI and the API disagreeing about which edits
//! are allowed. Only the presentation differs: the API answers a status and a
//! message, the pages answer a rendered form and an i18n key.

use crate::catalog::PermissionCatalog;
use quench_auth::prelude::{Permissions, Role, SessionDb, User};
use quench_db::prelude::{Crud, Db, Repository};
use std::sync::Arc;

/// Why an edit was refused.
///
/// Each variant knows its own HTTP status and its own translation key, so
/// neither caller has to keep a parallel table of them.
#[derive(Debug)]
pub enum RealmError {
    NotFound,
    UsernameEmpty,
    PasswordEmpty,
    AlreadyExists,
    /// Grants naming a service, or a `service:action` pair, the catalog does
    /// not recognise.
    UnknownGrants(Vec<String>),
    LastAdmin,
    SelfDemote,
    SelfDelete,
    UnknownTemplate,
    /// Assigning `admin` or `service` needs the literal `admin` role, not a
    /// catalog action - see `permissions.toml`'s comment on `[services.gatehouse]`.
    RolesRequireAdmin,
    Internal,
}

impl RealmError {
    pub const fn status(&self) -> actix_web::http::StatusCode {
        use actix_web::http::StatusCode;
        match self {
            Self::NotFound | Self::UnknownTemplate => StatusCode::NOT_FOUND,
            Self::UsernameEmpty | Self::PasswordEmpty | Self::UnknownGrants(_) => {
                StatusCode::BAD_REQUEST
            }
            Self::AlreadyExists | Self::LastAdmin | Self::SelfDemote | Self::SelfDelete => {
                StatusCode::CONFLICT
            }
            Self::RolesRequireAdmin => StatusCode::FORBIDDEN,
            Self::Internal => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn message(&self) -> String {
        match self {
            Self::NotFound => "no such user".to_string(),
            Self::UsernameEmpty => "username must not be empty".to_string(),
            Self::PasswordEmpty => "password must not be empty".to_string(),
            Self::AlreadyExists => "user already exists".to_string(),
            Self::UnknownGrants(grants) => {
                format!("unknown service or action: {}", grants.join(", "))
            }
            Self::LastAdmin => "the realm must keep at least one admin".to_string(),
            Self::SelfDemote => "you cannot remove your own admin role".to_string(),
            Self::SelfDelete => "you cannot delete your own account".to_string(),
            Self::UnknownTemplate => "no such permission template".to_string(),
            Self::RolesRequireAdmin => {
                "only an admin may assign the admin or service role".to_string()
            }
            Self::Internal => "the change could not be saved".to_string(),
        }
    }

    /// Fluent key for the admin pages. Kept alongside `message` rather than
    /// derived from it, so a reworded English string cannot silently change
    /// which translation is looked up.
    pub const fn i18n_key(&self) -> &'static str {
        match self {
            Self::NotFound => "ui_admin_error_not_found",
            Self::UsernameEmpty => "ui_admin_error_username_empty",
            Self::PasswordEmpty => "ui_admin_error_password_empty",
            Self::AlreadyExists => "ui_admin_error_exists",
            Self::UnknownGrants(_) => "ui_admin_error_unknown_service",
            Self::LastAdmin => "ui_admin_error_last_admin",
            Self::SelfDemote => "ui_admin_error_self_demote",
            Self::SelfDelete => "ui_admin_error_self_delete",
            Self::UnknownTemplate => "ui_admin_error_unknown_template",
            Self::RolesRequireAdmin => "ui_admin_error_roles_require_admin",
            Self::Internal => "ui_admin_error_internal",
        }
    }
}

pub type RealmResult<T> = Result<T, RealmError>;

/// What to change about a user. `None` leaves a field alone, so a form that only
/// touches permissions does not have to restate the password.
#[derive(Default)]
pub struct UserChanges {
    pub password: Option<String>,
    pub roles: Option<Vec<Role>>,
    pub permissions: Option<Permissions>,
}

impl UserChanges {
    /// Whether this changes what the subject may do, as opposed to only how they
    /// prove who they are. Decides whether their sessions end.
    const fn changes_access(&self) -> bool {
        self.roles.is_some() || self.permissions.is_some()
    }
}

fn repo(db: &Db) -> Repository<User> {
    db.repository::<User>()
}

fn internal(context: &str, err: impl std::fmt::Display) -> RealmError {
    tracing::error!("{context}: {err}");
    RealmError::Internal
}

/// Whether `roles` includes a wildcard role - `admin` or `service` - which
/// only the literal `admin` role may hand out. Catalog actions
/// (`gatehouse:create-user`, `gatehouse:edit-user`, ...) delegate everything
/// else about managing users, deliberately not this: granting a role that
/// itself grants everything is the one operation that must stay behind
/// `admin`, or `admin` stops being the emergency-only role it is meant to be.
fn wants_wildcard_role(roles: &[Role]) -> bool {
    roles
        .iter()
        .any(|role| matches!(role, Role::Admin | Role::Service))
}

pub async fn list(db: &Db) -> RealmResult<Vec<User>> {
    let mut users = repo(db)
        .list()
        .await
        .map_err(|err| internal("failed to list users", err))?;
    users.sort_by(|left, right| left.username.cmp(&right.username));
    Ok(users)
}

pub async fn get(db: &Db, username: &str) -> RealmResult<User> {
    repo(db)
        .read(username)
        .await
        .map_err(|err| internal("failed to read user", err))?
        .ok_or(RealmError::NotFound)
}

/// Rejects a grant naming a service, or a `service:action` pair, the catalog
/// does not recognise.
///
/// The catalog is already the ceiling for what a token's scope claim can say,
/// so such a grant could never take effect. Storing it silently would look to
/// an administrator like the grant had been saved.
fn check_grants(catalog: &PermissionCatalog, permissions: &Permissions) -> RealmResult<()> {
    let unknown = catalog.unknown_grants(permissions);
    if unknown.is_empty() {
        Ok(())
    } else {
        Err(RealmError::UnknownGrants(unknown))
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn create(
    db: &Db,
    catalog: &PermissionCatalog,
    actor_is_admin: bool,
    username: &str,
    password: &str,
    roles: Vec<Role>,
    permissions: Permissions,
    email: Option<String>,
) -> RealmResult<User> {
    let username = username.trim();
    if username.is_empty() {
        return Err(RealmError::UsernameEmpty);
    }
    if password.is_empty() {
        return Err(RealmError::PasswordEmpty);
    }
    if wants_wildcard_role(&roles) && !actor_is_admin {
        return Err(RealmError::RolesRequireAdmin);
    }
    check_grants(catalog, &permissions)?;

    let repo = repo(db);
    if repo
        .read(username)
        .await
        .map_err(|err| internal("failed to check for an existing user", err))?
        .is_some()
    {
        return Err(RealmError::AlreadyExists);
    }

    // A user with no role stated is an ordinary one. Defaulting to admin would
    // be the kind of convenience nobody notices until it matters.
    let roles = if roles.is_empty() {
        vec![Role::User]
    } else {
        roles
    };

    let user = User::new(
        username.to_string(),
        password.to_string(),
        roles,
        permissions,
        email,
    )
    .map_err(|err| internal("failed to hash the password", err))?;

    let created = repo
        .create(&user)
        .await
        .map_err(|err| internal("failed to create the user", err))?;
    tracing::info!("created user {username}");
    Ok(created)
}

/// Applies `changes`, holding the rules that keep the realm reachable.
///
/// `actor` is who is making the change: two of the rules are about acting on
/// yourself, and one of them is why a password change does not sign you out.
pub async fn update(
    db: &Db,
    catalog: &PermissionCatalog,
    sessions: &Arc<SessionDb>,
    actor: &str,
    actor_is_admin: bool,
    username: &str,
    changes: UserChanges,
) -> RealmResult<User> {
    let repo = repo(db);
    let mut user = get(db, username).await?;

    if let Some(permissions) = &changes.permissions {
        check_grants(catalog, permissions)?;
    }

    if let Some(roles) = &changes.roles {
        if wants_wildcard_role(roles) && !actor_is_admin {
            return Err(RealmError::RolesRequireAdmin);
        }
        let losing_admin = user.get_roles().contains(&Role::Admin) && !roles.contains(&Role::Admin);
        if losing_admin {
            if username == actor {
                return Err(RealmError::SelfDemote);
            }
            if last_admin(&repo, username).await? {
                return Err(RealmError::LastAdmin);
            }
        }
        user.roles = serde_json::to_value(roles).unwrap_or(user.roles);
    }

    if let Some(permissions) = &changes.permissions {
        user.permissions = serde_json::to_value(permissions).unwrap_or(user.permissions);
    }

    if let Some(password) = &changes.password {
        if password.is_empty() {
            return Err(RealmError::PasswordEmpty);
        }
        user.password = User::hash_password(password)
            .map_err(|err| internal("failed to hash password", err))?;
    }

    // Any change to what someone may do ends their sessions, so the new answer
    // applies now rather than when their access token expires. The exception is
    // an administrator changing only their own password: that should not sign
    // them out of the session they are changing it from.
    let revoke = changes.changes_access() || username != actor;

    let updated = repo
        .update(&user)
        .await
        .map_err(|err| internal("failed to update the user", err))?;

    if revoke {
        end_sessions(sessions, username).await;
    }
    tracing::info!("updated user {username}");
    Ok(updated)
}

pub async fn replace_permissions(
    db: &Db,
    catalog: &PermissionCatalog,
    sessions: &Arc<SessionDb>,
    actor: &str,
    username: &str,
    permissions: Permissions,
) -> RealmResult<User> {
    update(
        db,
        catalog,
        sessions,
        actor,
        // Never touches roles, so whether the actor holds `admin` cannot matter
        // here - see `wants_wildcard_role`, only consulted when `roles` is `Some`.
        false,
        username,
        UserChanges {
            permissions: Some(permissions),
            ..UserChanges::default()
        },
    )
    .await
}

/// Replaces `username`'s grants with a named template's, so an admin can
/// assign a bundle in one step instead of checking each box by hand.
pub async fn apply_template(
    db: &Db,
    catalog: &PermissionCatalog,
    sessions: &Arc<SessionDb>,
    actor: &str,
    username: &str,
    template: &str,
) -> RealmResult<User> {
    let grants = catalog
        .template(template)
        .cloned()
        .ok_or(RealmError::UnknownTemplate)?;
    replace_permissions(db, catalog, sessions, actor, username, grants).await
}

/// A self-registered account: always `Role::User`, always starts with the
/// catalog's default registration grants (§1.4's "an admin-created user
/// starts with nothing" does not apply here - there is no admin in the loop
/// to grant anything afterward, so a registered account that started with
/// nothing would simply be unusable until somebody happened to notice it).
pub async fn register(
    db: &Db,
    catalog: &PermissionCatalog,
    username: &str,
    password: &str,
    email: &str,
) -> RealmResult<User> {
    create(
        db,
        catalog,
        // Always `Role::User` below, so whether the actor holds `admin` cannot
        // matter here - see `wants_wildcard_role`.
        false,
        username,
        password,
        vec![Role::User],
        catalog.default_registration_grants(),
        Some(email.to_string()),
    )
    .await
}

/// Marks `username`'s email address confirmed. No session consequence -
/// confirming an address does not change what the account may do, unlike
/// every other write in this module.
pub async fn mark_email_verified(db: &Db, username: &str) -> RealmResult<()> {
    let repo = repo(db);
    let mut user = get(db, username).await?;
    user.email_verified_at = Some(chrono::Utc::now());
    repo.update(&user)
        .await
        .map_err(|err| internal("failed to record email verification", err))?;
    tracing::info!("verified email for {username}");
    Ok(())
}

/// Sets a new password after a reset link's token has already been redeemed -
/// the token proved control of the address, so this does not re-check the
/// old password the way a logged-in change would. Always ends every session:
/// unlike a logged-in password change, there is no "session you are changing
/// it from" to spare, and a reset is precisely the moment an account may have
/// been compromised.
pub async fn reset_password(
    db: &Db,
    sessions: &Arc<SessionDb>,
    username: &str,
    new_password: &str,
) -> RealmResult<()> {
    if new_password.is_empty() {
        return Err(RealmError::PasswordEmpty);
    }
    let repo = repo(db);
    let mut user = get(db, username).await?;
    user.password = User::hash_password(new_password)
        .map_err(|err| internal("failed to hash password", err))?;
    repo.update(&user)
        .await
        .map_err(|err| internal("failed to reset the password", err))?;
    end_sessions(sessions, username).await;
    tracing::info!("reset password for {username}");
    Ok(())
}

pub async fn delete(
    db: &Db,
    sessions: &Arc<SessionDb>,
    actor: &str,
    username: &str,
) -> RealmResult<()> {
    if username == actor {
        return Err(RealmError::SelfDelete);
    }

    let repo = repo(db);
    let user = get(db, username).await?;

    if user.get_roles().contains(&Role::Admin) && last_admin(&repo, username).await? {
        return Err(RealmError::LastAdmin);
    }

    repo.delete(username)
        .await
        .map_err(|err| internal("failed to delete the user", err))?;

    // Order matters: the row is gone, so any live session now belongs to nobody.
    // Leaving one would keep a deleted user signed in until their refresh token
    // expired.
    end_sessions(sessions, username).await;
    tracing::info!("deleted user {username}");
    Ok(())
}

/// Whether `excluding` is the only admin left.
///
/// `list` rather than a filtered query: `roles` is JSONB and the realm holds
/// people, not rows in the millions.
async fn last_admin(repo: &Repository<User>, excluding: &str) -> RealmResult<bool> {
    let users = repo
        .list()
        .await
        .map_err(|err| internal("failed to count admins", err))?;
    Ok(!users
        .iter()
        .any(|user| user.username != excluding && user.get_roles().contains(&Role::Admin)))
}

/// Best effort: failing to end sessions is worth a log, not a failed write that
/// leaves the caller unsure whether their change was saved.
async fn end_sessions(sessions: &Arc<SessionDb>, username: &str) {
    match sessions.revoke_all(username).await {
        Ok(0) => {}
        Ok(count) => tracing::info!("ended {count} session(s) for {username} after a change"),
        Err(err) => tracing::warn!("failed to end sessions for {username}: {err}"),
    }
}
