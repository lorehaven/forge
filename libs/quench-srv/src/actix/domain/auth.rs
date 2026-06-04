use quench_db::prelude::{Crud, Db, Model, Repository};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha512};
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
    pub fn new(username: String, password: String, roles: Vec<Role>) -> Self {
        Self {
            username,
            password: Self::hash_password(&password),
            roles: serde_json::to_value(roles).unwrap(),
        }
    }

    pub fn hash_password(password: &str) -> String {
        let mut hasher = Sha512::new();
        hasher.update(password.as_bytes());
        hex::encode(hasher.finalize())
    }

    pub fn get_roles(&self) -> Vec<Role> {
        serde_json::from_value(self.roles.clone()).unwrap_or_default()
    }
}

impl Model for User {
    fn table_name() -> String {
        let schema = envmnt::get_or("DB_SCHEMA", "public");
        format!("{}.users", schema)
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

impl UserDb {
    pub async fn init(db: Db) -> Arc<Self> {
        let repo = db.repository::<User>();
        let arc_db = Arc::new(Self::Base { repo });

        let auth_enabled = envmnt::get_or("SERVICE_AUTH_ENABLED", "false")
            .parse()
            .unwrap_or(false);

        if auth_enabled {
            let admin_user = envmnt::get_or_panic("SERVICE_USERNAME");
            let admin_pass = envmnt::get_or_panic("SERVICE_PASSWORD");
            let hashed_admin_pass = User::hash_password(&admin_pass);

            if let Some(user) = arc_db.get_user(&admin_user).await {
                if user.password != hashed_admin_pass {
                    arc_db
                        .add_user(User::new(admin_user, admin_pass, vec![Role::Admin]))
                        .await;
                }
            } else {
                arc_db
                    .add_user(User::new(admin_user, admin_pass, vec![Role::Admin]))
                    .await;
            }

            // Technical service user
            let tech_user = envmnt::get_or(
                "SERVICE_TECH_USERNAME",
                &envmnt::get_or("TECH_USERNAME", ""),
            );
            let tech_pass = envmnt::get_or(
                "SERVICE_TECH_PASSWORD",
                &envmnt::get_or("TECH_PASSWORD", ""),
            );

            if !tech_user.is_empty() && !tech_pass.is_empty() {
                let hashed_tech_pass = User::hash_password(&tech_pass);
                if let Some(user) = arc_db.get_user(&tech_user).await {
                    if user.password != hashed_tech_pass {
                        arc_db
                            .add_user(User::new(tech_user, tech_pass, vec![Role::Service]))
                            .await;
                    }
                } else {
                    arc_db
                        .add_user(User::new(tech_user, tech_pass, vec![Role::Service]))
                        .await;
                }
            }
        }

        arc_db
    }

    pub async fn add_user(&self, user: User) {
        let Self::Base { repo } = self;
        // Check if user exists to decide between create and update
        if repo.read(&user.username).await.unwrap_or(None).is_some() {
            repo.update(&user).await.ok();
        } else {
            repo.create(&user).await.ok();
        }
    }

    pub async fn get_user(&self, username: &str) -> Option<User> {
        let Self::Base { repo } = self;
        repo.read(username).await.unwrap_or(None)
    }

    pub async fn validate(&self, username: &str, password: &str) -> Option<User> {
        let user = self.get_user(username).await?;
        let hashed_password = User::hash_password(password);
        if user.password == hashed_password {
            Some(user)
        } else {
            None
        }
    }
}
