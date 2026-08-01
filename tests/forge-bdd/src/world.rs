//! One world for every suite.
//!
//! Cucumber allows a single `World` per run, so the per-service state that used
//! to live in three separate crates is unioned here. Generic steps ("GET
//! request is sent to …") resolve against [`ForgeWorld::target`], which each
//! feature's `Given <service> API is available` background step sets.

use cucumber::World;
use reqwest::Response;
use serde_json::Value;
use std::env;

/// Which service the generic steps talk to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Target {
    #[default]
    Sage,
    Switchboard,
    Warehouse,
    Gatehouse,
    Conveyor,
}

impl Target {
    /// Tag that selects this service's scenarios, and the name used on the CLI.
    pub const fn tag(self) -> &'static str {
        match self {
            Target::Sage => "sage",
            Target::Switchboard => "switchboard",
            Target::Warehouse => "warehouse",
            Target::Gatehouse => "gatehouse",
            Target::Conveyor => "conveyor",
        }
    }

    pub const ALL: [Target; 5] = [
        Target::Sage,
        Target::Switchboard,
        Target::Warehouse,
        Target::Gatehouse,
        Target::Conveyor,
    ];

    pub fn parse(value: &str) -> Option<Self> {
        Target::ALL
            .into_iter()
            .find(|target| target.tag() == value.trim_start_matches('@'))
    }
}

#[derive(Debug, World)]
#[world(init = Self::new)]
pub struct ForgeWorld {
    /// Service the generic steps address.
    pub target: Target,

    // Base URLs (host only) and API URLs (host + base path) per service.
    pub sage_base_url: String,
    pub sage_url: String,
    pub switchboard_base_url: String,
    pub switchboard_url: String,
    pub warehouse_base_url: String,
    pub warehouse_url: String,
    pub gatehouse_base_url: String,
    pub gatehouse_url: String,
    pub conveyor_base_url: String,
    pub conveyor_url: String,

    pub client: reqwest::Client,

    // Last response, shared by every suite.
    pub last_status: Option<u16>,
    pub last_json: Option<Value>,
    pub last_body: Option<String>,
    pub last_headers: reqwest::header::HeaderMap,
    pub last_response_headers: reqwest::header::HeaderMap,

    // sage
    pub jwt_token: String,
    pub current_conversation_id: Option<String>,

    // switchboard
    pub last_id: Option<String>,
    pub credentials: Option<(String, String)>,
    /// Bearer token switchboard requests present. Switchboard has no `UserDb`
    /// of its own (identity is federated through gatehouse-issued tokens), so
    /// unlike conveyor's `credentials`/basic-auth pair this is what actually
    /// authenticates it once `SERVICE_AUTH_ENABLED=true`. Defaults to a
    /// full-access token; `I hold a switchboard token scoped "..."` narrows it
    /// for a single scenario.
    pub switchboard_token: String,

    // warehouse
    pub docker_digest: Option<String>,
    pub token: Option<String>,
    pub username: String,
    pub password: String,
    pub current_crate_name: Option<String>,
    pub current_crate_version: Option<String>,

    // gatehouse
    pub session_cookie: Option<String>,
    pub refresh_cookie: Option<String>,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    /// Kept apart from `access_token` so a scenario can sign in as the user it
    /// just created and still administer the realm afterwards.
    pub admin_token: Option<String>,
}

impl ForgeWorld {
    pub async fn new() -> Self {
        let sage_base_url = service_url("SAGE_API_URL", "https://127.0.0.1:7777");
        let switchboard_base_url = service_url("SWITCHBOARD_API_URL", "https://127.0.0.1:8554");
        let warehouse_base_url = service_url("WAREHOUSE_API_URL", "https://127.0.0.1:8443");
        let gatehouse_base_url = service_url("GATEHOUSE_API_URL", "http://127.0.0.1:5443");
        let conveyor_base_url = service_url("CONVEYOR_API_URL", "http://127.0.0.1:9999");

        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        let gatehouse_url = format!("{gatehouse_base_url}/gatehouse");
        let jwt_token = mint_test_token(
            &client,
            &gatehouse_url,
            "test-user",
            &["sage"],
            "user sage:write",
        )
        .await;
        let switchboard_token = mint_test_token(
            &client,
            &gatehouse_url,
            "bdd-user",
            &["switchboard"],
            "admin",
        )
        .await;

        Self {
            target: Target::default(),
            sage_url: format!("{sage_base_url}/sage"),
            sage_base_url,
            switchboard_url: format!("{switchboard_base_url}/switchboard"),
            switchboard_base_url,
            warehouse_url: format!("{warehouse_base_url}/warehouse"),
            warehouse_base_url,
            gatehouse_url,
            gatehouse_base_url,
            conveyor_url: format!("{conveyor_base_url}/conveyor"),
            conveyor_base_url,
            client,
            last_status: None,
            last_json: None,
            last_body: None,
            last_headers: reqwest::header::HeaderMap::new(),
            last_response_headers: reqwest::header::HeaderMap::new(),
            jwt_token,
            current_conversation_id: None,
            last_id: None,
            credentials: None,
            switchboard_token,
            docker_digest: None,
            token: None,
            username: env::var("SERVICE_USERNAME").unwrap_or_else(|_| "admin".to_string()),
            password: env::var("SERVICE_PASSWORD").unwrap_or_else(|_| "password".to_string()),
            current_crate_name: None,
            current_crate_version: None,
            session_cookie: None,
            refresh_cookie: None,
            access_token: None,
            refresh_token: None,
            admin_token: None,
        }
    }

    /// Host + base path of the service currently under test.
    pub fn target_url(&self) -> &str {
        match self.target {
            Target::Sage => &self.sage_url,
            Target::Switchboard => &self.switchboard_url,
            Target::Warehouse => &self.warehouse_url,
            Target::Gatehouse => &self.gatehouse_url,
            Target::Conveyor => &self.conveyor_url,
        }
    }

    /// Host only, for routes that live outside the base path.
    pub fn target_base_url(&self) -> &str {
        match self.target {
            Target::Sage => &self.sage_base_url,
            Target::Switchboard => &self.switchboard_base_url,
            Target::Warehouse => &self.warehouse_base_url,
            Target::Gatehouse => &self.gatehouse_base_url,
            Target::Conveyor => &self.conveyor_base_url,
        }
    }

    /// Substitutes placeholders a scenario recorded earlier: the generated
    /// crate name (warehouse) and the id of the last created object
    /// (switchboard).
    pub fn resolve_placeholders(&self, path: &str) -> String {
        let mut resolved = path.to_string();
        if let Some(name) = &self.current_crate_name {
            for placeholder in [
                "test-crate-random-unique-xyz-789",
                "test-owners-crate",
                "test-index-crate",
                "test-crate-index-123",
            ] {
                resolved = resolved.replace(placeholder, name);
            }
        }
        if let Some(id) = &self.last_id {
            resolved = resolved.replace("{last_id}", id);
        }
        resolved
    }

    /// Full URL for a path against the current target. Warehouse serves its
    /// registry and index routes off the host root rather than the base path.
    pub fn resolve_url(&self, path: &str) -> String {
        let resolved = self.resolve_placeholders(path);
        let off_base_path = self.target == Target::Warehouse
            && !(resolved.starts_with("/api/v1/crates") || resolved.starts_with("/index"));

        if off_base_path {
            format!("{}{}", self.target_base_url(), resolved)
        } else {
            format!("{}{}", self.target_url(), resolved)
        }
    }

    pub fn apply_auth(&self, mut rb: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if self.target == Target::Switchboard {
            rb = rb.bearer_auth(&self.switchboard_token);
        } else if let Some((username, password)) = &self.credentials {
            rb = rb.basic_auth(username, Some(password));
        }
        rb
    }

    pub async fn record_response(&mut self, res: Response) {
        self.last_status = Some(res.status().as_u16());
        self.last_headers = res.headers().clone();
        self.last_response_headers = res.headers().clone();

        let content_type = res
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|h| h.to_str().ok())
            .unwrap_or("")
            .to_string();

        if content_type.contains("text/event-stream") {
            self.last_json = None;
            self.last_body = Some("SSE stream started".to_string());
            return;
        }

        if content_type.contains("application/json") {
            let json_text = res.text().await.ok().unwrap_or_default();
            let json: Option<Value> = serde_json::from_str(&json_text).ok();
            if let Some(json) = &json
                && let Some(id) = json.get("id").and_then(|value| value.as_str())
            {
                self.last_id = Some(id.to_string());
            }
            self.last_json = json;
            self.last_body = Some(json_text);
        } else {
            self.last_json = None;
            self.last_body = res.text().await.ok();
        }
    }
}

/// Mints a real, JWKS-verifiable token by asking gatehouse's
/// `POST /api/v1/test/token` (`docker/gatehouse-service/src/api/test_tokens.rs`,
/// enabled by `GATEHOUSE_TEST_MODE=true` - see `services.rs`) to sign one for
/// an arbitrary subject/audience/scope. No user or session has to exist for
/// this - the same shortcut the suite took when every service verified
/// against one shared HS256 secret, now routed through gatehouse's real
/// signing key so a relying party's JWKS-based verification accepts it.
pub async fn mint_test_token(
    client: &reqwest::Client,
    gatehouse_url: &str,
    sub: &str,
    aud: &[&str],
    scope: &str,
) -> String {
    mint_test_token_at(client, gatehouse_url, sub, aud, scope, None, None).await
}

/// Same, with explicit `iat`/`exp` (unix seconds) - what an expired-token or
/// future-`iat` scenario needs, since a real login can never produce either.
pub async fn mint_test_token_at(
    client: &reqwest::Client,
    gatehouse_url: &str,
    sub: &str,
    aud: &[&str],
    scope: &str,
    iat: Option<i64>,
    exp: Option<i64>,
) -> String {
    let response = client
        .post(format!("{gatehouse_url}/api/v1/test/token"))
        .json(&serde_json::json!({
            "sub": sub,
            "aud": aud,
            "scope": scope,
            "iat": iat,
            "exp": exp,
        }))
        .send()
        .await
        .unwrap_or_else(|e| panic!("failed to reach gatehouse's test token endpoint: {e}"));

    if !response.status().is_success() {
        panic!(
            "gatehouse refused to mint a test token (status {}) - is GATEHOUSE_TEST_MODE=true? {}",
            response.status(),
            response.text().await.unwrap_or_default()
        );
    }

    #[derive(serde::Deserialize)]
    struct TokenResponse {
        access_token: String,
    }
    response
        .json::<TokenResponse>()
        .await
        .expect("gatehouse's test token response was not what was expected")
        .access_token
}

fn service_url(key: &str, default: &str) -> String {
    env::var(key)
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default.to_string())
        .trim_end_matches('/')
        .to_string()
}
