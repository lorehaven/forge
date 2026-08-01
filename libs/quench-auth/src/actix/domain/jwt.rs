use crate::actix::domain::auth::{Permissions, Role};
use crate::actix::domain::jwks::JwksVerifier;
use async_trait::async_trait;
use jsonwebtoken::{
    Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, decode_header, encode,
    errors::ErrorKind,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Resolves the public key a token's `kid` was signed with.
///
/// Every service needs this to verify a token. Relying parties get here via
/// [`JwksVerifier`], which fetches and caches gatehouse's published keys over
/// HTTP; gatehouse resolves its own keys directly from what it holds, with no
/// network round trip - see `docker/gatehouse-service/src/keys.rs`.
#[async_trait]
pub trait KeyResolver: Send + Sync {
    async fn resolve(&self, kid: &str) -> Option<DecodingKey>;
}

/// Signs a token with the estate's current key.
///
/// Only gatehouse ever constructs a `JwtConfig` with a signer - every relying
/// party's `JwtConfig` has `signer: None` and can only verify.
#[async_trait]
pub trait KeySigner: Send + Sync {
    /// The `kid` and encoding key gatehouse currently signs new tokens with.
    async fn active(&self) -> Option<(String, EncodingKey)>;
}

#[derive(Clone)]
pub struct JwtConfig {
    pub service_name: String,
    /// Services a token issued here is valid for. Gatehouse lists the whole
    /// realm; a relying party lists only itself.
    pub audiences: Vec<String>,
    pub realm: String,
    pub auth_enabled: bool,
    pub access_token_ttl_secs: i64,
    pub refresh_token_ttl_secs: i64,
    keys: Arc<dyn KeyResolver>,
    signer: Option<Arc<dyn KeySigner>>,
}

impl JwtConfig {
    /// Relying-party construction: verifies tokens against the estate's
    /// published JWKS (`GATEHOUSE_URL`). Cannot sign - nothing but gatehouse
    /// ever needs to.
    pub async fn init() -> Self {
        let keys: Arc<dyn KeyResolver> = Arc::new(
            JwksVerifier::from_env()
                .await
                .expect("failed to set up the JWKS verifier (is GATEHOUSE_URL set?)"),
        );
        Self::from_parts(keys, None)
    }

    /// Gatehouse construction: `authority` resolves and signs with the same
    /// underlying key material, so a token gatehouse just minted verifies
    /// against itself without a round trip through its own HTTP endpoint.
    pub fn init_signing<T>(authority: Arc<T>) -> Self
    where
        T: KeyResolver + KeySigner + 'static,
    {
        let keys = authority.clone() as Arc<dyn KeyResolver>;
        let signer = authority as Arc<dyn KeySigner>;
        Self::from_parts(keys, Some(signer))
    }

    /// A `JwtConfig` that resolves and signs nothing - for tests that only
    /// need `service_name`/`audiences`/`access_token_ttl_secs`, never a real
    /// decode or encode. Public rather than `#[cfg(test)]`, since it is used
    /// from other crates' test binaries where a crate-local `cfg(test)` would
    /// not be visible.
    pub fn for_tests() -> Self {
        struct NoKeys;
        #[async_trait]
        impl KeyResolver for NoKeys {
            async fn resolve(&self, _kid: &str) -> Option<DecodingKey> {
                None
            }
        }
        Self::from_parts(Arc::new(NoKeys), None)
    }

    /// A `JwtConfig` backed by one freshly generated, in-memory Ed25519 key -
    /// for tests that need a real sign/verify round trip without a database.
    /// Gatehouse itself never uses this; see `docker/gatehouse-service/src/keys.rs`
    /// for the persisted, rotatable equivalent.
    pub fn for_tests_with_signing() -> Self {
        use crate::actix::domain::signing::{decoding_key, encoding_key, generate_signing_key};

        struct OneKey {
            kid: String,
            der: Vec<u8>,
            public: Vec<u8>,
        }
        #[async_trait]
        impl KeyResolver for OneKey {
            async fn resolve(&self, kid: &str) -> Option<DecodingKey> {
                (kid == self.kid).then(|| decoding_key(&self.public))
            }
        }
        #[async_trait]
        impl KeySigner for OneKey {
            async fn active(&self) -> Option<(String, EncodingKey)> {
                Some((self.kid.clone(), encoding_key(&self.der)))
            }
        }

        let generated = generate_signing_key();
        let authority = Arc::new(OneKey {
            kid: "test".to_string(),
            der: generated.private_key_der,
            public: generated.public_key,
        });
        Self::init_signing(authority)
    }

    fn from_parts(keys: Arc<dyn KeyResolver>, signer: Option<Arc<dyn KeySigner>>) -> Self {
        let service_name = envmnt::get_or("SERVICE_NAME", "service");
        let realm = envmnt::get_or("SERVICE_REALM", "https://localhost:8698/token");

        let audiences: Vec<String> = envmnt::get_or("SERVICE_AUDIENCES", "")
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect();
        let audiences = if audiences.is_empty() {
            vec![service_name.clone()]
        } else {
            audiences
        };

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
            service_name,
            audiences,
            realm,
            auth_enabled,
            access_token_ttl_secs,
            refresh_token_ttl_secs,
            keys,
            signer,
        }
    }

    /// Errs if this config has no signer - true for every service but
    /// gatehouse, which should never be asking to mint a token in the first
    /// place.
    pub async fn encode_claims(
        &self,
        claims: &Claims,
    ) -> Result<String, jsonwebtoken::errors::Error> {
        let signer = self
            .signer
            .as_ref()
            .ok_or_else(|| jsonwebtoken::errors::Error::from(ErrorKind::InvalidKeyFormat))?;
        let (kid, key) = signer
            .active()
            .await
            .ok_or_else(|| jsonwebtoken::errors::Error::from(ErrorKind::InvalidKeyFormat))?;

        let mut header = Header::new(Algorithm::EdDSA);
        header.kid = Some(kid);
        encode(&header, claims, &key)
    }

    pub async fn decode_claims(&self, token: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
        let header = decode_header(token)?;
        let kid = header
            .kid
            .ok_or_else(|| jsonwebtoken::errors::Error::from(ErrorKind::InvalidToken))?;
        let key = self
            .keys
            .resolve(&kid)
            .await
            .ok_or_else(|| jsonwebtoken::errors::Error::from(ErrorKind::InvalidKeyFormat))?;

        // Audience is checked against this service by `Claims::allows`, not
        // by `jsonwebtoken`'s own validator.
        let mut validation = Validation::new(Algorithm::EdDSA);
        validation.validate_aud = false;
        validation.required_spec_claims.insert("iat".to_string());
        validation.required_spec_claims.insert("exp".to_string());
        let token_data = decode::<Claims>(token, &key, &validation)?;

        let now = chrono::Utc::now().timestamp() as usize;
        if token_data.claims.iat > now + validation.leeway as usize {
            return Err(ErrorKind::ImmatureSignature.into());
        }

        Ok(token_data.claims)
    }

    /// Issues a token valid for every audience this config declares.
    pub async fn issue_access_token(
        &self,
        username: String,
        scope: String,
        session_id: Option<String>,
    ) -> Result<String, jsonwebtoken::errors::Error> {
        self.issue_access_token_for(username, self.audiences.clone(), scope, session_id)
            .await
    }

    /// Issues a token valid for `audiences` only.
    ///
    /// Gatehouse narrows the list to the services the subject may actually
    /// reach, which is what makes the audience check in the relying party's
    /// middleware enforce service access without the relying party knowing
    /// permissions exist. `self.audiences` stays the ceiling - see
    /// `narrow_audiences`.
    pub async fn issue_access_token_for(
        &self,
        username: String,
        audiences: Vec<String>,
        scope: String,
        session_id: Option<String>,
    ) -> Result<String, jsonwebtoken::errors::Error> {
        self.encode_claims(&Claims::for_audiences(
            username,
            audiences,
            scope,
            session_id,
            self.access_token_ttl_secs,
        ))
        .await
    }

    /// `wanted`, restricted to audiences this config declares.
    ///
    /// A caller cannot widen a token past `SERVICE_AUDIENCES` by asking for
    /// more, so a stale grant naming a service this deployment does not run
    /// cannot put that service in an audience list.
    pub fn narrow_audiences(&self, wanted: &[String]) -> Vec<String> {
        self.audiences
            .iter()
            .filter(|audience| wanted.iter().any(|want| want == *audience))
            .cloned()
            .collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,

    /// Services this token is valid for.
    #[serde(default)]
    pub aud: Vec<String>,

    pub scope: String,
    pub exp: usize,
    pub iat: usize,
    #[serde(default)]
    pub sid: Option<String>,
}

impl Claims {
    pub fn for_audiences(
        sub: String,
        aud: Vec<String>,
        scope: String,
        sid: Option<String>,
        duration_secs: i64,
    ) -> Self {
        let now = chrono::Utc::now();
        let iat = now.timestamp() as usize;
        let exp = (now + chrono::Duration::seconds(duration_secs)).timestamp() as usize;

        Self {
            sub,
            aud,
            scope,
            exp,
            iat,
            sid,
        }
    }

    /// Whether this token may be presented to `service_name`.
    pub fn allows(&self, service_name: &str) -> bool {
        self.aud.iter().any(|audience| audience == service_name)
    }

    /// Every entry in the scope claim, roles and permissions alike.
    pub fn roles(&self) -> Vec<String> {
        self.scope
            .split([' ', ','])
            .filter(|role| !role.is_empty())
            .map(str::to_string)
            .collect()
    }

    /// Whether the token carries a role.
    ///
    /// Token-wise, not a substring test. `scope.contains("admin")` would also
    /// match a permission whose service happened to be named `admin`; the
    /// vocabularies are kept disjoint (roles never contain a colon, permission
    /// levels are only `read`/`write`) so that cannot arise, and this makes it a
    /// guarantee rather than a convention.
    pub fn has_role(&self, role: &str) -> bool {
        self.roles()
            .iter()
            .any(|held| !held.contains(':') && held.eq_ignore_ascii_case(role))
    }

    /// The `service:action` entries in the scope claim, folded into
    /// `{service: {action, ...}}`. A service granted several actions carries
    /// one token per action (`sage:read sage:write`), not a combined one -
    /// the wire format stays a flat list of space-separated tokens either way.
    pub fn permissions(&self) -> Permissions {
        let mut result = Permissions::new();
        for entry in self.roles() {
            if let Some((service, action)) = entry.split_once(':') {
                result
                    .entry(service.to_string())
                    .or_default()
                    .insert(action.to_string());
            }
        }
        result
    }

    /// Whether the token permits `action` on `service`.
    ///
    /// A wildcard role short-circuits, which is why an admin's token carries no
    /// permission entries at all.
    pub fn can(&self, service: &str, action: &str) -> bool {
        if self.has_wildcard() {
            return true;
        }
        self.permissions()
            .get(service)
            .is_some_and(|actions| actions.contains(action))
    }

    /// Whether any role on the token grants everything.
    pub fn has_wildcard(&self) -> bool {
        self.roles().iter().any(|held| {
            !held.contains(':') && Role::parse(held).is_some_and(|role| role.is_wildcard())
        })
    }
}
