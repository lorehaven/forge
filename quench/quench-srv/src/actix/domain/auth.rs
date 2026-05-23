use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Role {
    Admin,
    User,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub username: String,
    pub password: String,
    pub roles: Vec<Role>,
}

pub struct UserDb {
    users: RwLock<HashMap<String, User>>,
}

impl Default for UserDb {
    fn default() -> Self {
        Self::new()
    }
}

impl UserDb {
    pub fn new() -> Self {
        Self {
            users: RwLock::new(HashMap::new()),
        }
    }

    pub fn init() -> Arc<Self> {
        let db = Arc::new(Self::default());

        let auth_enabled = envmnt::get_or("SERVICE_AUTH_ENABLED", "false")
            .parse()
            .unwrap_or(false);

        if auth_enabled {
            let admin_user = envmnt::get_or_panic("SERVICE_USERNAME");
            let admin_pass = envmnt::get_or_panic("SERVICE_PASSWORD");

            db.add_user(User {
                username: admin_user,
                password: admin_pass,
                roles: vec![Role::Admin],
            });
        }

        db
    }

    pub fn add_user(&self, user: User) {
        let mut users = self.users.write().unwrap();
        users.insert(user.username.clone(), user);
    }

    pub fn get_user(&self, username: &str) -> Option<User> {
        let users = self.users.read().unwrap();
        users.get(username).cloned()
    }

    pub fn validate(&self, username: &str, password: &str) -> Option<User> {
        let user = self.get_user(username)?;
        if user.password == password {
            Some(user)
        } else {
            None
        }
    }
}
