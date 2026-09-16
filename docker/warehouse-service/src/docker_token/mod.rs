//! Docker Registry v2 bearer tokens: Basic-auth at `/token`, bearer at `/v2/*` - a fixed protocol.
//! Minted/verified by warehouse alone with its own secret, independent of the realm's JWKS tokens.

use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode, errors::Error};
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct DockerTokenConfig {
    pub secret: Vec<u8>,
    pub service_name: String,
    pub realm: String,
    pub auth_enabled: bool,
}

impl DockerTokenConfig {
    pub fn init(service_name: String, realm: String, auth_enabled: bool) -> Self {
        let secret = envmnt::get_or_panic("DOCKER_TOKEN_SECRET").into_bytes();
        Self {
            secret,
            service_name,
            realm,
            auth_enabled,
        }
    }

    pub fn encode(&self, claims: &DockerClaims) -> Result<String, Error> {
        encode(
            &Header::default(),
            claims,
            &EncodingKey::from_secret(&self.secret),
        )
    }

    pub fn decode(&self, token: &str) -> Result<DockerClaims, Error> {
        let validation = Validation {
            validate_aud: false,
            ..Validation::default()
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
