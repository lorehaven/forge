use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng},
};
use quench_db::prelude::{Crud, Db, Model, Repository};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, sqlx::Type)]
#[sqlx(rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Admin,
    User,
    Service,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct User {
    pub username: String,
    pub password: String,
    pub roles: serde_json::Value,
}

impl User {
    pub fn new(username: String, password: String, roles: Vec<Role>) -> anyhow::Result<Self> {
        Ok(Self {
            username,
            password: Self::hash_password(&password)?,
            roles: serde_json::to_value(roles).unwrap(),
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
}

impl Model for User {
    fn table_name() -> String {
        format!("{}.users", crate::actix::domain::realm::auth_schema())
    }

    fn columns() -> Vec<&'static str> {
        vec!["username", "password", "roles"]
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
