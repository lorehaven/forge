use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct JwtConfig {
    pub jwt_secret: Vec<u8>,
    pub service_name: String,
    pub realm: String,
    pub auth_enabled: bool,
}

impl JwtConfig {
    pub fn init() -> Self {
        let jwt_secret = envmnt::get_or_panic("JWT_SECRET").into_bytes();
        let service_name = envmnt::get_or("SERVICE_NAME", "service");
        let realm = envmnt::get_or("SERVICE_REALM", "https://localhost:8698/token");

        let auth_enabled = envmnt::get_or("SERVICE_AUTH_ENABLED", "false")
            .parse()
            .unwrap_or(false);

        Self {
            jwt_secret,
            service_name,
            realm,
            auth_enabled,
        }
    }

    pub fn encode_claims(&self, claims: &Claims) -> Result<String, jsonwebtoken::errors::Error> {
        encode(
            &Header::default(),
            claims,
            &EncodingKey::from_secret(&self.jwt_secret),
        )
    }

    pub fn decode_claims(&self, token: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
        let mut validation = Validation::default();
        validation.validate_exp = true;
        let token_data = decode::<Claims>(
            token,
            &DecodingKey::from_secret(&self.jwt_secret),
            &validation,
        )?;
        Ok(token_data.claims)
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

impl Claims {
    pub fn new(sub: String, service: String, scope: String, duration_secs: i64) -> Self {
        let now = chrono::Utc::now();
        let iat = now.timestamp() as usize;
        let exp = (now + chrono::Duration::seconds(duration_secs)).timestamp() as usize;

        Self {
            sub,
            service,
            scope,
            exp,
            iat,
        }
    }
}
