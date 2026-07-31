use crate::actix::domain::auth::{Permissions, Role};
use jsonwebtoken::{
    DecodingKey, EncodingKey, Header, Validation, decode, encode, errors::ErrorKind,
};
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct JwtConfig {
    pub jwt_secret: Vec<u8>,
    pub service_name: String,
    /// Services a token issued here is valid for. Gatehouse lists the whole
    /// realm; a relying party lists only itself.
    pub audiences: Vec<String>,
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
            jwt_secret,
            service_name,
            audiences,
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
        // Audience is checked against this service by `Claims::allows`, which
        // also honours the legacy single-`service` claim.
        let mut validation = Validation {
            validate_aud: false,
            validate_exp: true,
            ..Validation::default()
        };
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

    /// Issues a token valid for every audience this config declares.
    pub fn issue_access_token(
        &self,
        username: String,
        scope: String,
        session_id: Option<String>,
    ) -> Result<String, jsonwebtoken::errors::Error> {
        self.issue_access_token_for(username, self.audiences.clone(), scope, session_id)
    }

    /// Issues a token valid for `audiences` only.
    ///
    /// Gatehouse narrows the list to the services the subject may actually
    /// reach, which is what makes the audience check in the relying party's
    /// middleware enforce service access without the relying party knowing
    /// permissions exist. `self.audiences` stays the ceiling - see
    /// `narrow_audiences`.
    pub fn issue_access_token_for(
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

    /// Single-service audience as issued before the shared realm. Kept so
    /// tokens minted by the previous release stay valid for one rollout;
    /// remove once every service is on 0.2.
    pub service: String,

    pub scope: String,
    pub exp: usize,
    pub iat: usize,
    #[serde(default)]
    pub sid: Option<String>,
}

impl Claims {
    /// Single-audience token - the pre-realm shape, still used by the
    /// machine-to-machine Basic auth path.
    pub fn new(
        sub: String,
        service: String,
        scope: String,
        sid: Option<String>,
        duration_secs: i64,
    ) -> Self {
        Self::for_audiences(sub, vec![service], scope, sid, duration_secs)
    }

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
            service: aud.first().cloned().unwrap_or_default(),
            aud,
            scope,
            exp,
            iat,
            sid,
        }
    }

    /// Whether this token may be presented to `service_name`.
    pub fn allows(&self, service_name: &str) -> bool {
        self.aud.iter().any(|audience| audience == service_name) || self.service == service_name
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
