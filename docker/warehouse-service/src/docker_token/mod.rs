//! Docker Registry v2 bearer tokens: Basic-auth at `/token`, bearer at `/v2/*` - a fixed protocol.
//! Minted/verified by warehouse alone with its own secret, independent of the realm's JWKS tokens.

use quench_crypto::jwt::{
    Algorithm, DecodingKey, EncodingKey, Error, Header, Validation, decode, encode,
};
use quench_serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct DockerTokenConfig {
    pub secret: Vec<u8>,
    pub service_name: String,
    pub realm: String,
    pub auth_enabled: bool,
}

impl DockerTokenConfig {
    #[must_use]
    pub fn init(service_name: String, realm: String, auth_enabled: bool) -> Self {
        let secret = std::env::var("DOCKER_TOKEN_SECRET")
            .expect("DOCKER_TOKEN_SECRET must be set")
            .into_bytes();
        Self {
            secret,
            service_name,
            realm,
            auth_enabled,
        }
    }

    pub fn encode(&self, claims: &DockerClaims) -> Result<String, Error> {
        encode(
            &Header::new(Algorithm::HS256),
            claims,
            &EncodingKey::from_secret(&self.secret),
        )
    }

    pub fn decode(&self, token: &str) -> Result<DockerClaims, Error> {
        let validation = Validation {
            validate_aud: false,
            ..Validation::new(Algorithm::HS256)
        };
        Ok(
            decode::<DockerClaims>(token, &DecodingKey::from_secret(&self.secret), &validation)?
                .claims,
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DockerClaims {
    pub sub: String,
    pub service: String,
    pub scope: String,
    pub exp: usize,
    pub iat: usize,
}
