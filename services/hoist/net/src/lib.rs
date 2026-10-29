//! Hoist's talk to the services: gatehouse for the session, warehouse for the registry. Plain blocking calls
//! (run them on a worker thread); no interface and no host: the native renderer's apps are Rust programs and call
//! the services themselves, so the page's security policy and the services' lack of CORS do not come into it.
//!
//! Tokens: the password is used once, by [`login`]; [`Session`] keeps the access token in memory and the refresh
//! token for the caller to store (the refresh rotates: store the new one **before** the next request, which
//! [`Session::refresh_token`] hands back after every refresh).

use std::fmt;
use std::io::Write;
use std::path::{Path, PathBuf};

use hoist_core::{Artifact, Category, PublicApp, PublicPage, PublicQuery};
use quench_client::blocking::Client;
use quench_serde::{Deserialize, Value, json};

/// What went wrong, in the terms a screen shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetError {
    /// The services could not be reached (or answered something unreadable).
    Network(String),
    /// The user name or password is wrong, or the refresh token is no longer good.
    Unauthorized,
    /// The account has a second factor, which hoist cannot ask for: turn it off for this account.
    MfaRequired,
    /// The server has no registry (a `404` on the catalog).
    RegistryDisabled,
    /// The server has no public catalogue (a `404` on the catalogue service).
    PublicDisabled,
    /// The file that arrived is not the one the registry described (it was deleted).
    HashMismatch,
    /// Any other answer from the server.
    Status(u16),
}

impl fmt::Display for NetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Network(why) => write!(f, "cannot reach the server: {why}"),
            Self::Unauthorized => {
                f.write_str("the user name or password is wrong, or the session ended")
            }
            Self::MfaRequired => {
                f.write_str("this account needs a second factor: turn MFA off for it to use hoist")
            }
            Self::RegistryDisabled => f.write_str("this server has no registry"),
            Self::PublicDisabled => f.write_str("this server has no public catalogue"),
            Self::HashMismatch => f.write_str("the download was corrupted, try again"),
            Self::Status(code) => write!(f, "the server answered {code}"),
        }
    }
}

impl std::error::Error for NetError {}

fn network(e: impl fmt::Display) -> NetError {
    NetError::Network(e.to_string())
}

/// The tokens gatehouse gives.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct Tokens {
    access_token: String,
    refresh_token: String,
    #[serde(default)]
    expires_in: i64,
}

/// A signed-in session against one server.
#[derive(Debug, Clone)]
pub struct Session {
    base: String,
    access: String,
    refresh: String,
    expires_in: i64,
}

fn base_of(url: &str) -> String {
    url.trim().trim_end_matches('/').to_string()
}

fn post_json(client: &Client, url: &str, body: &Value) -> Result<(u16, String), NetError> {
    let response = client.post(url).json(body).send().map_err(network)?;
    let status = response.status().as_u16();
    Ok((status, response.text().map_err(network)?))
}

fn tokens(base: &str, status: u16, text: &str) -> Result<Session, NetError> {
    match status {
        200 => {
            let t: Tokens = json::from_str(text).map_err(network)?;
            Ok(Session {
                base: base.to_string(),
                access: t.access_token,
                refresh: t.refresh_token,
                expires_in: t.expires_in,
            })
        }
        401 | 403 if text.contains("mfa_required") => Err(NetError::MfaRequired),
        400..=403 if text.contains("mfa_required") => Err(NetError::MfaRequired),
        400 | 401 | 403 => Err(NetError::Unauthorized),
        other => Err(NetError::Status(other)),
    }
}

/// Signs in. The password is not kept.
///
/// # Errors
/// [`NetError`].
pub fn login(base: &str, username: &str, password: &str) -> Result<Session, NetError> {
    let base = base_of(base);
    let body = json!({ "username": username, "password": password });
    let (status, text) = post_json(
        &Client::new(),
        &format!("{base}/gatehouse/api/v1/auth/login"),
        &body,
    )?;
    tokens(&base, status, &text)
}

impl Session {
    /// A session from a refresh token kept from an earlier one (use [`refresh`](Self::refresh) before the first
    /// request).
    #[must_use]
    pub fn resume(base: &str, refresh_token: &str) -> Self {
        Self {
            base: base_of(base),
            access: String::new(),
            refresh: refresh_token.to_string(),
            expires_in: 0,
        }
    }

    /// The token to store: the new one after every refresh.
    #[must_use]
    pub fn refresh_token(&self) -> &str {
        &self.refresh
    }

    /// Seconds the access token lives (refresh at about 80% of it).
    #[must_use]
    pub const fn expires_in(&self) -> i64 {
        self.expires_in
    }

    /// Gets a new access token (and a new refresh token: the old one is spent).
    ///
    /// # Errors
    /// [`NetError::Unauthorized`] when the refresh token is no good: sign in again.
    pub fn refresh(&mut self) -> Result<(), NetError> {
        let body = json!({ "refresh_token": self.refresh });
        let (status, text) = post_json(
            &Client::new(),
            &format!("{}/gatehouse/api/v1/auth/refresh", self.base),
            &body,
        )?;
        *self = tokens(&self.base, status, &text)?;
        Ok(())
    }

    /// A `GET` with the bearer token; on a `401` the token is refreshed and the request made once more.
    fn get(&mut self, path: &str) -> Result<quench_client::blocking::Response, NetError> {
        self.get_with(path, &[])
    }

    fn get_with(
        &mut self,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<quench_client::blocking::Response, NetError> {
        for attempt in 0..2 {
            if self.access.is_empty() && attempt == 0 {
                self.refresh()?;
            }
            let response = Client::new()
                .get(format!("{}{path}", self.base))
                .query(query)
                .bearer_auth(&self.access)
                .send()
                .map_err(network)?;
            if response.status().as_u16() == 401 && attempt == 0 {
                self.refresh()?;
                continue;
            }
            return Ok(response);
        }
        Err(NetError::Unauthorized)
    }

    /// The registry's latest builds for `platform` (`android`, `linux`...).
    ///
    /// # Errors
    /// [`NetError::RegistryDisabled`] for a `404`; the rest as [`NetError`].
    pub fn catalog(&mut self, platform: &str) -> Result<Vec<Artifact>, NetError> {
        let response = self.get(&format!("/warehouse/api/v1/artifacts?platform={platform}"))?;
        match response.status().as_u16() {
            200 => {}
            404 => return Err(NetError::RegistryDisabled),
            401 | 403 => return Err(NetError::Unauthorized),
            other => return Err(NetError::Status(other)),
        }
        let value: Value = response.json().map_err(network)?;
        // A list, or an object that holds one.
        let rows = match &value {
            Value::Array(_) => value.clone(),
            other => other
                .get("items")
                .or_else(|| other.get("artifacts"))
                .cloned()
                .ok_or_else(|| NetError::Network("the catalog is not a list".into()))?,
        };
        json::from_str(&json::to_string(&rows)).map_err(network)
    }

    /// Downloads a build into `dir` and checks its SHA-256 against the registry's: a file that does not match is
    /// deleted. `progress` is told how many bytes of how many have arrived (the file arrives whole, so it is
    /// called once at the end here).
    ///
    /// # Errors
    /// [`NetError::HashMismatch`] when the file is not the one described; the rest as [`NetError`].
    pub fn download(
        &mut self,
        artifact: &Artifact,
        dir: &Path,
        mut progress: impl FnMut(u64, u64),
    ) -> Result<PathBuf, NetError> {
        let path = format!(
            "/warehouse/api/v1/artifacts/{}/{}/{}/download?format={}",
            artifact.program, artifact.platform, artifact.version_code, artifact.format
        );
        let response = self.get(&path)?;
        match response.status().as_u16() {
            200 => {}
            401 | 403 => return Err(NetError::Unauthorized),
            other => return Err(NetError::Status(other)),
        }
        let bytes = response.bytes().map_err(network)?;
        let total = bytes.len() as u64;
        let digest: String = quench_crypto::hash::sha256(&bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        if !digest.eq_ignore_ascii_case(&artifact.sha256) {
            return Err(NetError::HashMismatch);
        }
        std::fs::create_dir_all(dir).map_err(network)?;
        // Only the file's own name: nothing the server sends can place it elsewhere.
        let name = Path::new(&artifact.filename)
            .file_name()
            .and_then(|n| n.to_str())
            .filter(|n| !n.is_empty() && *n != "..")
            .ok_or_else(|| NetError::Network("the file has no usable name".into()))?;
        let target = dir.join(name);
        let mut file = std::fs::File::create(&target).map_err(network)?;
        file.write_all(&bytes).map_err(network)?;
        progress(total, total);
        Ok(target)
    }

    /// A page of the public catalogue (the catalogue service): browsed, or searched when `query` has text.
    ///
    /// # Errors
    /// [`NetError::PublicDisabled`] for a `404`; the rest as [`NetError`].
    pub fn public_apps(&mut self, q: &PublicQuery) -> Result<PublicPage, NetError> {
        let mut pairs = vec![
            (
                "sort",
                if q.sort.is_empty() {
                    "downloads"
                } else {
                    &q.sort
                }
                .to_string(),
            ),
            ("offset", q.offset.max(0).to_string()),
            ("limit", q.limit.clamp(1, 100).to_string()),
        ];
        if let Some(text) = q.query.as_deref().filter(|t| !t.trim().is_empty()) {
            pairs.push(("query", text.trim().to_string()));
        }
        if let Some(category) = q.category.as_deref().filter(|c| !c.is_empty()) {
            pairs.push(("category", category.to_string()));
        }
        if let Some(votes) = q.min_votes {
            pairs.push(("min_votes", votes.to_string()));
        }
        let response = self.get_with("/hoist/v1/apps", &pairs)?;
        public_answer(response.status().as_u16())?;
        response.json().map_err(network)
    }

    /// The categories of the public catalogue, the biggest first.
    ///
    /// # Errors
    /// As [`public_apps`](Self::public_apps).
    pub fn public_categories(&mut self) -> Result<Vec<Category>, NetError> {
        #[derive(Deserialize)]
        struct List {
            items: Vec<Category>,
        }
        let response = self.get("/hoist/v1/categories")?;
        public_answer(response.status().as_u16())?;
        let list: List = response.json().map_err(network)?;
        Ok(list.items)
    }

    /// Downloads an app of the public catalogue into `dir`. The file comes from the store the catalogue names, not
    /// from this server, so **no token is sent**; it must be `https`. When the catalogue has a SHA-256 the file is
    /// checked against it and deleted if it differs; without one it is kept unverified (the system still checks
    /// the app's signature when it installs).
    ///
    /// # Errors
    /// [`NetError::HashMismatch`] when the file is not the one described; the rest as [`NetError`].
    pub fn download_public(
        app: &PublicApp,
        dir: &Path,
        mut progress: impl FnMut(u64, u64),
    ) -> Result<PathBuf, NetError> {
        if !is_secure(&app.download_url) {
            return Err(NetError::Network(
                "the download is not offered over https".into(),
            ));
        }
        let name = format!("{}-{}.apk", app.package_name, app.version_code);
        if !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
            || name.starts_with('.')
        {
            return Err(NetError::Network("the package name is not usable".into()));
        }
        let response = Client::new()
            .get(&app.download_url)
            .send()
            .map_err(network)?;
        match response.status().as_u16() {
            200 => {}
            other => return Err(NetError::Status(other)),
        }
        let bytes = response.bytes().map_err(network)?;
        let total = bytes.len() as u64;
        if let Some(want) = app.sha256.as_deref().filter(|h| !h.is_empty()) {
            let digest: String = quench_crypto::hash::sha256(&bytes)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            if !digest.eq_ignore_ascii_case(want) {
                return Err(NetError::HashMismatch);
            }
        }
        std::fs::create_dir_all(dir).map_err(network)?;
        let target = dir.join(name);
        std::fs::write(&target, &bytes).map_err(network)?;
        progress(total, total);
        Ok(target)
    }
}

/// Whether a download address may be fetched: `https`, or a loopback address (which never leaves the machine).
fn is_secure(url: &str) -> bool {
    url.starts_with("https://")
        || ["http://127.0.0.1", "http://localhost", "http://[::1]"]
            .iter()
            .any(|p| {
                url.strip_prefix(p)
                    .is_some_and(|rest| rest.is_empty() || rest.starts_with([':', '/']))
            })
}

/// What a status from the catalogue service means.
fn public_answer(status: u16) -> Result<(), NetError> {
    match status {
        200 => Ok(()),
        404 => Err(NetError::PublicDisabled),
        401 | 403 => Err(NetError::Unauthorized),
        other => Err(NetError::Status(other)),
    }
}
