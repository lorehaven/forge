//! Rules for changing realm users, shared by `api::users` and `ui::pages::admin`
//! so the two surfaces can't disagree about which edits are allowed.

use crate::catalog::PermissionCatalog;
use quench_auth::prelude::{Permissions, Role, SessionDb, User};
use quench_db::prelude::{Crud, Db, Repository};
use std::sync::Arc;

/// Why an edit was refused - each variant carries its own status and i18n key.
#[derive(Debug)]
pub enum RealmError {
    NotFound,
    UsernameEmpty,
    PasswordEmpty,
    AlreadyExists,
    /// Service or `service:action` pair the catalog doesn't recognise.
    UnknownGrants(Vec<String>),
    LastAdmin,
    SelfDemote,
    SelfDelete,
    /// Would lock you out with no other admin able to undo it.
    SelfDisable,
    UnknownTemplate,
    /// Assigning admin/service needs the literal `admin` role, not a catalog action.
    RolesRequireAdmin,
    /// Enrollment code didn't match - MFA stays off until one succeeds.
    MfaCodeInvalid,
    Internal,
}

impl RealmError {
    pub const fn status(&self) -> http::StatusCode {
        use http::StatusCode;
        match self {
            Self::NotFound | Self::UnknownTemplate => StatusCode::NOT_FOUND,
            Self::UsernameEmpty | Self::PasswordEmpty | Self::UnknownGrants(_) => {
                StatusCode::BAD_REQUEST
            }
            Self::AlreadyExists
            | Self::LastAdmin
            | Self::SelfDemote
            | Self::SelfDelete
            | Self::SelfDisable => StatusCode::CONFLICT,
            Self::RolesRequireAdmin => StatusCode::FORBIDDEN,
            Self::MfaCodeInvalid => StatusCode::BAD_REQUEST,
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
            Self::SelfDisable => "you cannot disable your own account".to_string(),
            Self::UnknownTemplate => "no such permission template".to_string(),
            Self::RolesRequireAdmin => {
                "only an admin may assign the admin or service role".to_string()
            }
            Self::MfaCodeInvalid => "that code did not match - try again".to_string(),
            Self::Internal => "the change could not be saved".to_string(),
        }
    }

    /// Kept alongside `message`, not derived from it, so rewording English
    /// text can't silently change which translation is looked up.
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
            Self::SelfDisable => "ui_admin_error_self_disable",
            Self::UnknownTemplate => "ui_admin_error_unknown_template",
            Self::RolesRequireAdmin => "ui_admin_error_roles_require_admin",
            Self::MfaCodeInvalid => "ui_admin_error_mfa_code_invalid",
            Self::Internal => "ui_admin_error_internal",
        }
    }
}

pub type RealmResult<T> = Result<T, RealmError>;

/// What to change about a user. `None` leaves a field alone.
#[derive(Default)]
pub struct UserChanges {
    pub password: Option<String>,
    pub roles: Option<Vec<Role>>,
    pub permissions: Option<Permissions>,
    // Profile fields: blank-to-clear isn't supported, matching `password`'s rule.
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub title: Option<String>,
    pub timezone: Option<String>,
    pub preferred_locale: Option<String>,
}

impl UserChanges {
    /// Whether this changes access (vs. only identity) - decides session revocation.
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

/// Whether `roles` includes admin/service - only the literal `admin` role may
/// grant those, kept out of the delegable catalog actions on purpose.
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

/// Rejects a grant the catalog doesn't recognise - it could never take
/// effect, and storing it silently would look like it had been saved.
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

    // No role stated defaults to plain user, never admin.
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
        user.password_changed_at = Some(chrono::Utc::now());
    }

    if let Some(display_name) = &changes.display_name {
        user.display_name = Some(display_name.clone());
    }
    if let Some(avatar_url) = &changes.avatar_url {
        user.avatar_url = Some(avatar_url.clone());
    }
    if let Some(title) = &changes.title {
        user.title = Some(title.clone());
    }
    if let Some(timezone) = &changes.timezone {
        user.timezone = Some(timezone.clone());
    }
    if let Some(preferred_locale) = &changes.preferred_locale {
        user.preferred_locale = Some(preferred_locale.clone());
    }

    // Access changes end sessions immediately, except an admin's own password.
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
        // Never touches roles, so `actor_is_admin` is irrelevant here.
        false,
        username,
        UserChanges {
            permissions: Some(permissions),
            ..UserChanges::default()
        },
    )
    .await
}

/// Replaces `username`'s grants with a named template's.
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

/// Self-registered account: always `Role::User`, starts with the catalog's
/// default grants (no admin in the loop to grant anything afterward).
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
        // Always `Role::User` below, so `actor_is_admin` is irrelevant here.
        false,
        username,
        password,
        vec![Role::User],
        catalog.default_registration_grants(),
        Some(email.to_string()),
    )
    .await
}

/// Marks the email confirmed. No session consequence - doesn't change access.
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

/// New password after a redeemed reset token - no old-password check, and
/// always ends every session since the account may have been compromised.
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
    user.password_changed_at = Some(chrono::Utc::now());
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

    // Row is gone first, so a live session now belongs to nobody.
    end_sessions(sessions, username).await;
    tracing::info!("deleted user {username}");
    Ok(())
}

// --- Login ---

/// What happened when checking a login attempt.
pub enum AuthOutcome {
    /// Boxed - `User` is large enough next to the data-free variants here to
    /// trip clippy's `large_enum_variant` otherwise.
    Success(Box<User>),
    NotFound,
    Disabled,
    Locked,
    WrongPassword,
    /// Password right, MFA enabled - `pending` is a short-lived signed token
    /// proving this step happened, carried through to [`authenticate_mfa`].
    MfaRequired {
        pending: String,
    },
}

/// Wrong-password threshold and lockout duration, configurable via
/// `GATEHOUSE_LOGIN_MAX_ATTEMPTS`/`GATEHOUSE_LOCKOUT_DURATION_SECS`.
fn lockout_policy() -> (i32, chrono::Duration) {
    let max_attempts = envmnt::get_or("GATEHOUSE_LOGIN_MAX_ATTEMPTS", "5")
        .parse()
        .unwrap_or(5);
    let lockout_secs = envmnt::get_or("GATEHOUSE_LOCKOUT_DURATION_SECS", "900")
        .parse()
        .unwrap_or(900);
    (max_attempts, chrono::Duration::seconds(lockout_secs))
}

/// Gatehouse's own login (write access) - stops short of a session when MFA is enabled.
pub async fn authenticate(db: &Db, username: &str, password: &str) -> RealmResult<AuthOutcome> {
    let repo = repo(db);
    let Some(mut user) = repo
        .read(username)
        .await
        .map_err(|err| internal("failed to look up user for login", err))?
    else {
        return Ok(AuthOutcome::NotFound);
    };

    if user.is_disabled() {
        return Ok(AuthOutcome::Disabled);
    }
    if user.is_locked() {
        return Ok(AuthOutcome::Locked);
    }

    let verify_user = user.clone();
    let plain_password = password.to_string();
    let verified =
        tokio::task::spawn_blocking(move || verify_user.verify_password(&plain_password))
            .await
            .unwrap_or(false);

    if !verified {
        let (max_attempts, lockout_duration) = lockout_policy();
        user.record_failed_login(max_attempts, lockout_duration);
        repo.update(&user)
            .await
            .map_err(|err| internal("failed to record a failed login", err))?;
        return Ok(AuthOutcome::WrongPassword);
    }

    if user.mfa_enabled {
        let pending = crate::mfa::sign_pending(&user.username)
            .map_err(|err| internal("failed to sign a pending MFA token", err))?;
        return Ok(AuthOutcome::MfaRequired { pending });
    }

    user.record_successful_login();
    let updated = repo
        .update(&user)
        .await
        .map_err(|err| internal("failed to record a successful login", err))?;
    Ok(AuthOutcome::Success(Box::new(updated)))
}

/// Second login step when MFA is enabled - checks the code, finishing what
/// `authenticate` started. A wrong code counts toward the same lockout.
pub async fn authenticate_mfa(db: &Db, pending: &str, code: &str) -> RealmResult<AuthOutcome> {
    let Some(username) = crate::mfa::verify_pending(pending) else {
        // Expired or tampered - same generic error either way.
        return Ok(AuthOutcome::WrongPassword);
    };

    let repo = repo(db);
    let Some(mut user) = repo
        .read(&username)
        .await
        .map_err(|err| internal("failed to look up user for MFA check", err))?
    else {
        return Ok(AuthOutcome::NotFound);
    };

    if user.is_disabled() {
        return Ok(AuthOutcome::Disabled);
    }
    if user.is_locked() {
        return Ok(AuthOutcome::Locked);
    }

    let Some(secret) = user.mfa_secret.as_deref() else {
        // MFA turned off mid-flow - fail rather than skip the promised check.
        return Ok(AuthOutcome::WrongPassword);
    };
    let decrypted = crate::mfa::decrypt_secret(secret)
        .map_err(|err| internal("failed to decrypt MFA secret", err))?;

    if !crate::mfa::verify_code(&decrypted, code) {
        let (max_attempts, lockout_duration) = lockout_policy();
        user.record_failed_login(max_attempts, lockout_duration);
        repo.update(&user)
            .await
            .map_err(|err| internal("failed to record a failed MFA attempt", err))?;
        return Ok(AuthOutcome::WrongPassword);
    }

    user.record_successful_login();
    let updated = repo
        .update(&user)
        .await
        .map_err(|err| internal("failed to record a successful login", err))?;
    Ok(AuthOutcome::Success(Box::new(updated)))
}

// --- MFA enrollment ---

/// Fresh secret, not yet persisted - an abandoned enrollment leaves no trace.
pub fn begin_mfa_enrollment(username: &str) -> anyhow::Result<(String, String)> {
    let secret = crate::mfa::generate_secret()?;
    let uri = crate::mfa::provisioning_uri(&secret, username)?;
    Ok((secret, uri))
}

/// Turns MFA on once the caller proves the secret was saved correctly.
pub async fn enable_mfa(db: &Db, username: &str, secret: &str, code: &str) -> RealmResult<()> {
    if !crate::mfa::verify_code(secret, code) {
        return Err(RealmError::MfaCodeInvalid);
    }
    let repo = repo(db);
    let mut user = get(db, username).await?;
    let encrypted = crate::mfa::encrypt_secret(secret)
        .map_err(|err| internal("failed to encrypt MFA secret", err))?;
    user.mfa_enabled = true;
    user.mfa_secret = Some(encrypted);
    repo.update(&user)
        .await
        .map_err(|err| internal("failed to enable MFA", err))?;
    tracing::info!("enabled MFA for {username}");
    Ok(())
}

/// Turns MFA off - self-service or admin recovery.
pub async fn disable_mfa(db: &Db, username: &str) -> RealmResult<()> {
    let repo = repo(db);
    let mut user = get(db, username).await?;
    user.mfa_enabled = false;
    user.mfa_secret = None;
    repo.update(&user)
        .await
        .map_err(|err| internal("failed to disable MFA", err))?;
    tracing::info!("disabled MFA for {username}");
    Ok(())
}

// --- Admin lifecycle actions ---

/// Unlike `update`, doesn't end sessions - disabling only blocks future logins.
pub async fn set_disabled(db: &Db, username: &str, disabled: bool) -> RealmResult<User> {
    let repo = repo(db);
    let mut user = get(db, username).await?;
    user.disabled_at = disabled.then(chrono::Utc::now);
    let updated = repo
        .update(&user)
        .await
        .map_err(|err| internal("failed to change disabled state", err))?;
    tracing::info!(
        "{} {username}",
        if disabled { "disabled" } else { "enabled" }
    );
    Ok(updated)
}

/// Clears the failed-attempt counter and lock together, so login starts clean.
pub async fn unlock(db: &Db, username: &str) -> RealmResult<User> {
    let repo = repo(db);
    let mut user = get(db, username).await?;
    user.locked_until = None;
    user.failed_login_attempts = 0;
    let updated = repo
        .update(&user)
        .await
        .map_err(|err| internal("failed to unlock the user", err))?;
    tracing::info!("unlocked {username}");
    Ok(updated)
}

/// Whether `excluding` is the only admin left. Full `list`, not a filtered
/// query - `roles` is JSONB and the realm holds people, not millions of rows.
async fn last_admin(repo: &Repository<User>, excluding: &str) -> RealmResult<bool> {
    let users = repo
        .list()
        .await
        .map_err(|err| internal("failed to count admins", err))?;
    Ok(!users
        .iter()
        .any(|user| user.username != excluding && user.get_roles().contains(&Role::Admin)))
}

/// Best effort - failing to end sessions is a log, not a failed write.
async fn end_sessions(sessions: &Arc<SessionDb>, username: &str) {
    match sessions.revoke_all(username).await {
        Ok(0) => {}
        Ok(count) => tracing::info!("ended {count} session(s) for {username} after a change"),
        Err(err) => tracing::warn!("failed to end sessions for {username}: {err}"),
    }
}
