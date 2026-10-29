//! Hoist's talk to the services: gatehouse for the session (the shared `gatehouse-client`), warehouse for the
//! registry. Plain blocking calls (run them on a worker thread); no interface and no host: the native renderer's apps
//! are Rust programs and call the services themselves, so the page's security policy and the services' lack of CORS
//! do not come into it.
//!
//! Tokens: the password is used once, by [`login`]; [`Session`] keeps the access token in memory and the refresh
//! token for the caller to store (the refresh rotates: store the new one **before** the next request, which
//! [`Session::keeping_tokens`] does).

use std::fmt;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use gatehouse_client::transfer_client;
use hoist_core::{Artifact, Category, PublicApp, PublicPage, PublicQuery};
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
    /// Too many wrong passwords: gatehouse refuses this account for a while, even with the right one.
    AccountLocked,
    /// An administrator turned this account off.
    AccountDisabled,
    /// The account's email address was never confirmed.
    EmailUnverified,
    /// The public catalogue's service turned the (valid) session away. Not a sign-out: the registry is still open.
    PublicRefused(u16),
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
            Self::AccountLocked => f.write_str(
                "this account is locked after too many wrong passwords: try again in about 15 minutes, or have it unlocked in gatehouse",
            ),
            Self::AccountDisabled => f.write_str("this account is disabled"),
            Self::EmailUnverified => {
                f.write_str("confirm the email address of this account first (gatehouse sent a link)")
            }
            Self::PublicRefused(code) => {
                write!(f, "the public catalogue is not available ({code})")
            }
        }
    }
}

impl std::error::Error for NetError {}

fn network(e: impl fmt::Display) -> NetError {
    NetError::Network(e.to_string())
}

impl From<gatehouse_client::Error> for NetError {
    fn from(e: gatehouse_client::Error) -> Self {
        use gatehouse_client::Error as E;
        match e {
            E::Network(why) => Self::Network(why),
            E::Unauthorized => Self::Unauthorized,
            E::MfaRequired => Self::MfaRequired,
            E::Status(code) => Self::Status(code),
            E::AccountLocked => Self::AccountLocked,
            E::AccountDisabled => Self::AccountDisabled,
            E::EmailUnverified => Self::EmailUnverified,
        }
    }
}

/// How long a call to a service may take (the whole call), and how long a file may take to arrive. Set once and
/// before the first call (the first call fixes them). A test uses short ones.
pub fn set_timeouts(call: Duration, transfer: Duration) {
    gatehouse_client::set_timeouts(call, transfer);
}

/// Signs in. The password is not kept.
///
/// # Errors
/// [`NetError`].
pub fn login(base: &str, username: &str, password: &str) -> Result<Session, NetError> {
    gatehouse_client::trace_var("HOIST_TRACE");
    Ok(Session(gatehouse_client::login(base, username, password)?))
}

/// A signed-in session against one server.
#[derive(Clone, Debug)]
pub struct Session(gatehouse_client::Session);

impl Session {
    /// A session from a refresh token kept from an earlier one (use [`refresh`](Self::refresh) before the first
    /// request).
    #[must_use]
    pub fn resume(base: &str, refresh_token: &str) -> Self {
        gatehouse_client::trace_var("HOIST_TRACE");
        Self(gatehouse_client::Session::resume(base, refresh_token))
    }

    /// The token to store: the new one after every refresh.
    #[must_use]
    pub fn refresh_token(&self) -> &str {
        self.0.refresh_token()
    }

    /// Seconds the access token lives (refresh at about 80% of it).
    #[must_use]
    pub const fn expires_in(&self) -> i64 {
        self.0.expires_in()
    }

    /// Calls `store` with every new refresh token, as soon as it is given. See
    /// [`gatehouse_client::Session::keeping_tokens`].
    #[must_use]
    pub fn keeping_tokens(self, store: impl Fn(&str) + Send + Sync + 'static) -> Self {
        Self(self.0.keeping_tokens(store))
    }

    /// Asks `latest` for the newest refresh token kept by anyone before a refresh. See
    /// [`gatehouse_client::Session::following`].
    #[must_use]
    pub fn following(self, latest: impl Fn() -> Option<String> + Send + Sync + 'static) -> Self {
        Self(self.0.following(latest))
    }

    /// Gets a new access token (and a new refresh token: the old one is spent).
    ///
    /// # Errors
    /// [`NetError::Unauthorized`] when the refresh token is no good: sign in again.
    pub fn refresh(&mut self) -> Result<(), NetError> {
        Ok(self.0.refresh()?)
    }

    fn get(&mut self, path: &str) -> Result<gatehouse_client::Response, NetError> {
        self.get_with(path, &[])
    }

    fn get_with(
        &mut self,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<gatehouse_client::Response, NetError> {
        // A file is allowed its time; everything else must answer.
        Ok(self.0.get(path, query, path.contains("/download"))?)
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
        let response = transfer_client()
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
        // The session is good (the registry took it): the public catalogue's own service says no. Signing the user
        // out of everything for that would be wrong.
        401 | 403 => Err(NetError::PublicRefused(status)),
        other => Err(NetError::Status(other)),
    }
}
