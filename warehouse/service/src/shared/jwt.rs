use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct JwtConfig {
    pub jwt_secret: Vec<u8>,
    pub service_name: String,
    pub realm: String,
    pub auth_enabled: bool,
    pub username: Option<String>,
    pub password: Option<String>,
}

impl JwtConfig {
    pub fn init() -> Self {
        let jwt_secret = envmnt::get_or_panic("JWT_SECRET").into_bytes();
        let service_name = envmnt::get_or("REGISTRY_SERVICE", "warehouse");
        let realm = envmnt::get_or("REGISTRY_REALM", "https://localhost:8698/token");

        let auth_enabled = envmnt::get_or("REGISTRY_AUTH_ENABLED", "false")
            .parse()
            .unwrap_or(false);
        let (username, password) = if auth_enabled {
            (
                Some(envmnt::get_or_panic("REGISTRY_USERNAME")),
                Some(envmnt::get_or_panic("REGISTRY_PASSWORD")),
            )
        } else {
            (None, None)
        };

        Self {
            jwt_secret,
            service_name,
            realm,
            auth_enabled,
            username,
            password,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub service: String,
    pub scope: String,
    pub exp: usize,
    pub iat: usize,
}
