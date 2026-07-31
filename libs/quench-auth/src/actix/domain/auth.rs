use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng},
};
use quench_db::prelude::{Crud, Db, Model, Repository};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, sqlx::Type)]
#[sqlx(rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Admin,
    User,
    Service,
}

impl Role {
    /// Whether the role grants every permission on every service.
    ///
    /// `Admin` administers the realm; `Service` is the machine-to-machine
    /// identity, which has to reach whatever the estate happens to contain.
    /// Both are wildcards so that adding a service never means re-granting
    /// anything, and neither has entries written into `permissions`: an
    /// enumerated grant list would go stale the moment `SERVICE_AUDIENCES`
    /// changed.
    ///
    /// Note this is about *service access* only. Managing users is `Admin`
    /// alone - see gatehouse's `AdminClaims`.
    pub const fn is_wildcard(&self) -> bool {
        matches!(self, Self::Admin | Self::Service)
    }

    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Admin => "admin",
            Self::User => "user",
            Self::Service => "service",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "admin" => Some(Self::Admin),
            "user" => Some(Self::User),
            "service" => Some(Self::Service),
            _ => None,
        }
    }
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The set of action names a service declares it supports.
///
/// A grant is holding one of these names for a service - `"read"`, `"write"`,
/// or something a service defines for itself, like switchboard's `"launch"`.
/// There is deliberately no ordering or implication between them (`"write"`
/// does not imply `"read"`): the catalog that defines which names exist per
/// service (`docker/gatehouse-service/config/permissions.toml`) is what makes
/// this safe - an admin checks the boxes they mean, and there is nothing
/// magic underneath for a reader to get wrong. `quench-auth` has no notion of
/// which action names exist for which service; that lives in gatehouse's
/// catalog, and is checked at grant time, not at read time.
pub type Actions = BTreeSet<String>;

/// A user's grants, as `{service: {action, action, ...}}`.
pub type Permissions = BTreeMap<String, Actions>;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct User {
    pub username: String,
    pub password: String,
    pub roles: serde_json::Value,
    /// `{"sage": ["read", "write"], "switchboard": ["launch"]}`. Empty for a
    /// wildcard role, which grants everything without enumerating it.
    pub permissions: serde_json::Value,
    /// Set by self-service registration; `None` for an admin-created account,
    /// which has no reason to collect one.
    pub email: Option<String>,
    /// When the address in `email` was confirmed via a verification link.
    /// `None` either because there is no address, or because it has not been
    /// confirmed yet - the two are told apart by checking `email` itself.
    pub email_verified_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl User {
    pub fn new(
        username: String,
        password: String,
        roles: Vec<Role>,
        permissions: Permissions,
        email: Option<String>,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            username,
            password: Self::hash_password(&password)?,
            roles: serde_json::to_value(roles).unwrap(),
            permissions: serde_json::to_value(permissions).unwrap(),
            email,
            email_verified_at: None,
        })
    }

    pub fn hash_password(password: &str) -> anyhow::Result<String> {
        let salt = SaltString::generate(&mut OsRng);
        Ok(Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map_err(|err| anyhow::anyhow!("failed to hash password: {err}"))?
            .to_string())
    }

    pub fn verify_password(&self, password: &str) -> bool {
        let Ok(hash) = PasswordHash::new(&self.password) else {
            return false;
        };
        Argon2::default()
            .verify_password(password.as_bytes(), &hash)
            .is_ok()
    }

    pub fn get_roles(&self) -> Vec<Role> {
        serde_json::from_value(self.roles.clone()).unwrap_or_default()
    }

    /// The grants written against this user, ignoring any wildcard role.
    ///
    /// This is what an editor shows and what a `PUT` replaces. For "may they
    /// actually do X", use [`User::can`] - it is the one that honours the
    /// wildcard. A row that will not parse as `{service: [action, ...]}`
    /// reads as no grants at all rather than failing the read: one bad row
    /// should not lock a user out of everything, and "no access" is the safe
    /// direction for that failure to go.
    pub fn get_permissions(&self) -> Permissions {
        serde_json::from_value(self.permissions.clone()).unwrap_or_default()
    }

    pub fn has_wildcard(&self) -> bool {
        self.get_roles().iter().any(Role::is_wildcard)
    }

    /// Whether this user may perform `action` on `service`. A wildcard role
    /// short-circuits, which is why `get_permissions` stays empty for one -
    /// there is nothing to enumerate.
    pub fn can(&self, service: &str, action: &str) -> bool {
        if self.has_wildcard() {
            return true;
        }
        self.get_permissions()
            .get(service)
            .is_some_and(|actions| actions.contains(action))
    }
}

impl Model for User {
    fn table_name() -> String {
        format!("{}.users", crate::actix::domain::realm::auth_schema())
    }

    fn columns() -> Vec<&'static str> {
        vec![
            "username",
            "password",
            "roles",
            "permissions",
            "email",
            "email_verified_at",
        ]
    }

    fn primary_key_name() -> String {
        "username".to_string()
    }
}

pub enum UserDb {
    Base { repo: Repository<User> },
}

/// Read-only access to the realm's users.
///
/// Relying parties need this for the machine-to-machine Basic auth path and,
/// in warehouse's case, the registry token endpoint. Creating users is
/// gatehouse's job, so there is deliberately no write method here.
impl UserDb {
    pub async fn init(db: Db) -> Arc<Self> {
        Arc::new(Self::Base {
            repo: db.repository::<User>(),
        })
    }

    pub async fn get_user(&self, username: &str) -> Option<User> {
        let Self::Base { repo } = self;
        repo.read(username).await.unwrap_or(None)
    }

    pub async fn validate(&self, username: &str, password: &str) -> Option<User> {
        tracing::debug!("UserDb::validate: Looking up user: {}", username);
        let user = self.get_user(username).await?;
        tracing::debug!(
            "UserDb::validate: User found, verifying password for: {}",
            username
        );
        let verify_user = user.clone();
        let password = password.to_string();
        let verified = tokio::task::spawn_blocking(move || verify_user.verify_password(&password))
            .await
            .ok()?;
        if verified {
            tracing::debug!("UserDb::validate: Password verified for user: {}", username);
            Some(user)
        } else {
            tracing::warn!("UserDb::validate: Invalid password for user: {}", username);
            None
        }
    }
}
