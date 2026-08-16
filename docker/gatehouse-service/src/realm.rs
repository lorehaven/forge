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
    /// Disabling yourself would lock you out with no other admin able to
    /// undo it from inside the realm - the same reasoning `SelfDelete` gives.
    SelfDisable,
    UnknownTemplate,
    /// Assigning `admin` or `service` needs the literal `admin` role, not a
    /// catalog action - see `permissions.toml`'s comment on `[services.gatehouse]`.
    RolesRequireAdmin,
    /// The code offered at MFA enrollment did not match the secret just
    /// generated - enrollment does not turn MFA on until this succeeds once,
    /// so a mistyped code just means try again, not a half-enabled account.
    MfaCodeInvalid,
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
            Self::SelfDisable => "ui_admin_error_self_disable",
            Self::UnknownTemplate => "ui_admin_error_unknown_template",
            Self::RolesRequireAdmin => "ui_admin_error_roles_require_admin",
            Self::MfaCodeInvalid => "ui_admin_error_mfa_code_invalid",
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
    // Profile - self-service and admin-editable alike go through this same
    // struct, same reasoning the module doc gives for password/roles/
    // permissions: one path, so the two callers can't disagree about what's
    // allowed. Blank-to-clear isn't supported (matching `password`'s own
    // "empty means leave alone" rule) - a real limitation, not an oversight,
    // traded for not needing a second "explicitly clear this" signal.
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub title: Option<String>,
    pub timezone: Option<String>,
    pub preferred_locale: Option<String>,
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

    // Order matters: the row is gone, so any live session now belongs to nobody.
    // Leaving one would keep a deleted user signed in until their refresh token
    // expired.
    end_sessions(sessions, username).await;
    tracing::info!("deleted user {username}");
    Ok(())
}

// ---------------------------------------------------------------------------
// Login
// ---------------------------------------------------------------------------

/// What happened when checking a login attempt.
pub enum AuthOutcome {
    /// Password (and, if this account has MFA, the code) checked out.
    /// Carries the updated row - `last_login_at` stamped,
    /// `failed_login_attempts` reset.
    Success(User),
    NotFound,
    Disabled,
    Locked,
    WrongPassword,
    /// The password was right, but this account has MFA enabled - a session
    /// is not issued yet. `pending` is a short-lived signed token
    /// (`mfa::sign_pending`) proving this step already happened; the caller
    /// carries it through the code-entry form and back to
    /// [`authenticate_mfa`].
    MfaRequired { pending: String },
}

/// How many wrong passwords in a row locks an account, and for how long.
/// Configurable (`GATEHOUSE_LOGIN_MAX_ATTEMPTS`, default 5;
/// `GATEHOUSE_LOCKOUT_DURATION_SECS`, default 900) rather than fixed in
/// `quench-auth` - `User::record_failed_login` takes both as plain
/// parameters and has no opinion of its own about what they should be.
fn lockout_policy() -> (i32, chrono::Duration) {
    let max_attempts = envmnt::get_or("GATEHOUSE_LOGIN_MAX_ATTEMPTS", "5")
        .parse()
        .unwrap_or(5);
    let lockout_secs = envmnt::get_or("GATEHOUSE_LOCKOUT_DURATION_SECS", "900")
        .parse()
        .unwrap_or(900);
    (max_attempts, chrono::Duration::seconds(lockout_secs))
}

/// Checks a login attempt: existence, disabled/locked state, the password,
/// and - if this account has MFA enabled - stops one step short of a session
/// so the caller can send the browser to the code-entry page instead.
///
/// Distinct from `quench_auth::UserDb::validate`, which every relying party
/// also uses for its own machine-to-machine Basic auth path: that one has no
/// write access to track any of this with, by design - see its own doc
/// comment. This is gatehouse's own interactive login, which does.
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
    Ok(AuthOutcome::Success(updated))
}

/// The second step of a login when MFA is enabled - `pending` proves the
/// password was already checked (see `authenticate`'s `MfaRequired`), so
/// this only has to check the code and finish what `authenticate` started.
/// A wrong code counts toward the same lockout a wrong password would.
pub async fn authenticate_mfa(db: &Db, pending: &str, code: &str) -> RealmResult<AuthOutcome> {
    let Some(username) = crate::mfa::verify_pending(pending) else {
        // Expired or tampered - back to square one rather than a more
        // specific error that would tell an attacker which.
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
        // MFA was turned off between the password step and this one - fail
        // rather than silently skip a check that was already promised.
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
    Ok(AuthOutcome::Success(updated))
}

// ---------------------------------------------------------------------------
// MFA enrollment
// ---------------------------------------------------------------------------

/// Starts enrollment: a fresh secret, not yet saved anywhere - the caller
/// shows it (as an otpauth URI for a QR code, and the raw secret for manual
/// entry) and asks for one correct code before [`enable_mfa`] actually turns
/// it on. Nothing is persisted here, on purpose: an abandoned enrollment
/// leaves no trace.
pub fn begin_mfa_enrollment(username: &str) -> anyhow::Result<(String, String)> {
    let secret = crate::mfa::generate_secret()?;
    let uri = crate::mfa::provisioning_uri(&secret, username)?;
    Ok((secret, uri))
}

/// Turns MFA on, once the caller has proven the user actually saved the
/// secret by producing one correct code for it.
pub async fn enable_mfa(db: &Db, username: &str, secret: &str, code: &str) -> RealmResult<()> {
    if !crate::mfa::verify_code(secret, code) {
        return Err(RealmError::MfaCodeInvalid);
    }
    let repo = repo(db);
    let mut user = get(db, username).await?;
    let encrypted =
        crate::mfa::encrypt_secret(secret).map_err(|err| internal("failed to encrypt MFA secret", err))?;
    user.mfa_enabled = true;
    user.mfa_secret = Some(encrypted);
    repo.update(&user)
        .await
        .map_err(|err| internal("failed to enable MFA", err))?;
    tracing::info!("enabled MFA for {username}");
    Ok(())
}

/// Turns MFA off - from the account's own self-service page (with a fresh
/// code) or by an admin, for recovery when a user has lost their
/// authenticator.
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

// ---------------------------------------------------------------------------
// Admin lifecycle actions
// ---------------------------------------------------------------------------

/// An admin turning an account on or off. Unlike `update`, this does not end
/// the account's sessions - disabling only stops a *future* login, the same
/// way `UserDb::validate`/`authenticate` check it, and re-enabling shouldn't
/// need a fresh sign-in either.
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

/// A support "unlock" after a lockout - clears the counter and the lock
/// together, so the next login attempt starts clean rather than one attempt
/// away from re-locking.
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
