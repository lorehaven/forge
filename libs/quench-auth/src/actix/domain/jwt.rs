use jsonwebtoken::{
    DecodingKey, EncodingKey, Header, Validation, decode, encode, errors::ErrorKind,
};
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct JwtConfig {
    pub jwt_secret: Vec<u8>,
    pub service_name: String,
    pub realm: String,
    pub auth_enabled: bool,
    pub access_token_ttl_secs: i64,
    pub refresh_token_ttl_secs: i64,
}

impl JwtConfig {
    pub fn init() -> Self {
        let jwt_secret = envmnt::get_or_panic("JWT_SECRET").into_bytes();
        let service_name = envmnt::get_or("SERVICE_NAME", "service");
        let realm = envmnt::get_or("SERVICE_REALM", "https://localhost:8698/token");

        let auth_enabled = envmnt::get_or("SERVICE_AUTH_ENABLED", "false")
            .parse()
            .unwrap_or(false);
        let access_token_ttl_secs = envmnt::get_or("ACCESS_TOKEN_TTL_SECS", "900")
            .parse()
            .unwrap_or(900);
        let refresh_token_ttl_secs = envmnt::get_or("REFRESH_TOKEN_TTL_SECS", "604800")
            .parse()
            .unwrap_or(604800);

        Self {
            jwt_secret,
            service_name,
            realm,
            auth_enabled,
            access_token_ttl_secs,
            refresh_token_ttl_secs,
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
        validation.required_spec_claims.insert("iat".to_string());
        validation.required_spec_claims.insert("exp".to_string());
        let token_data = decode::<Claims>(
            token,
            &DecodingKey::from_secret(&self.jwt_secret),
            &validation,
        )?;

        let now = chrono::Utc::now().timestamp() as usize;
        if token_data.claims.iat > now + validation.leeway as usize {
            return Err(ErrorKind::ImmatureSignature.into());
        }

        Ok(token_data.claims)
    }

    pub fn issue_access_token(
        &self,
        username: String,
        scope: String,
        session_id: Option<String>,
    ) -> Result<String, jsonwebtoken::errors::Error> {
        self.encode_claims(&Claims::new(
            username,
            self.service_name.clone(),
            scope,
            session_id,
            self.access_token_ttl_secs,
        ))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub service: String,
    pub scope: String,
    pub exp: usize,
    pub iat: usize,
    #[serde(default)]
    pub sid: Option<String>,
}

impl Claims {
    pub fn new(
        sub: String,
        service: String,
        scope: String,
        sid: Option<String>,
        duration_secs: i64,
    ) -> Self {
        let now = chrono::Utc::now();
        let iat = now.timestamp() as usize;
        let exp = (now + chrono::Duration::seconds(duration_secs)).timestamp() as usize;

        Self {
            sub,
            service,
            scope,
            exp,
            iat,
            sid,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use envmnt;

    #[test]
    fn test_jwt_expiration() {
        envmnt::set("JWT_SECRET", "test_secret");
        let config = JwtConfig::init();

        let claims = Claims::new(
            "user".to_string(),
            "service".to_string(),
            "scope".to_string(),
            None,
            -300,
        ); // Expired 5 minutes ago
        let token = config.encode_claims(&claims).unwrap();

        let result = config.decode_claims(&token);
        assert!(
            result.is_err(),
            "Expired token should be rejected: {:?}",
            result
        );
    }

    #[test]
    fn test_jwt_iat_future() {
        envmnt::set("JWT_SECRET", "test_secret");
        let config = JwtConfig::init();

        let now = chrono::Utc::now();
        let iat = (now + chrono::Duration::seconds(300)).timestamp() as usize; // Issued 5 minutes in the future
        let exp = (now + chrono::Duration::seconds(600)).timestamp() as usize;

        let claims = Claims {
            sub: "user".to_string(),
            service: "service".to_string(),
            scope: "scope".to_string(),
            exp,
            iat,
            sid: None,
        };

        let token = config.encode_claims(&claims).unwrap();

        let result = config.decode_claims(&token);
        assert!(
            result.is_err(),
            "Token with future iat should be rejected: {:?}",
            result
        );
    }
}
