//! Forklift's talk to the services: gatehouse for the session (the shared `gatehouse-client`), warehouse for the
//! storage. Plain blocking calls (run them on a worker thread). Files are not sent from here on Android: the host
//! streams them from the media library with the token this session hands out ([`Session::fresh_token`]);
//! [`Session::put`] is for the hosts that can read a file themselves.
//!
//! Tokens: the password is used once, by [`login`]; the refresh token rotates, so it is handed to the caller to keep
//! **before** the next request (see [`Session::keeping_tokens`] and [`Session::following`]).

pub mod store;

pub use store::{RunLock, Store};

use std::fmt;
use std::time::Duration;

use quench_serde::Value;

/// What went wrong, in the terms a screen shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetError {
    /// The services could not be reached (or answered something unreadable).
    Network(String),
    /// The user name or password is wrong, or the refresh token is no longer good.
    Unauthorized,
    /// The account has a second factor, which forklift cannot ask for: turn it off for this account.
    MfaRequired,
    /// Any other answer from the server.
    Status(u16),
    /// Too many wrong passwords: gatehouse refuses this account for a while, even with the right one.
    AccountLocked,
    /// An administrator turned this account off.
    AccountDisabled,
    /// The account's email address was never confirmed.
    EmailUnverified,
    /// The server has no storage with that name.
    NoStorage,
    /// The account is known, but may not do this with the storage (it needs a grant in gatehouse).
    Forbidden,
    /// The file is larger than the server accepts.
    TooLarge,
    /// The storage is full.
    QuotaFull,
}

impl fmt::Display for NetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Network(why) => write!(f, "cannot reach the server: {why}"),
            Self::Unauthorized => {
                f.write_str("the user name or password is wrong, or the session ended")
            }
            Self::MfaRequired => {
                f.write_str("this account needs a second factor: turn MFA off for it to use forklift")
            }
            Self::Status(code) => write!(f, "the server answered {code}"),
            Self::AccountLocked => f.write_str(
                "this account is locked after too many wrong passwords: try again in about 15 minutes, or have it unlocked in gatehouse",
            ),
            Self::AccountDisabled => f.write_str("this account is disabled"),
            Self::NoStorage => f.write_str("the server has no storage with that name"),
            Self::Forbidden => f.write_str(
                "this account is not allowed to do that with the storage: ask for access to it in gatehouse",
            ),
            Self::TooLarge => f.write_str("the file is larger than the server accepts"),
            Self::QuotaFull => f.write_str("the storage is full"),
            Self::EmailUnverified => {
                f.write_str("confirm the email address of this account first (gatehouse sent a link)")
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
    gatehouse_client::trace_var("FORKLIFT_TRACE");
    Ok(Session(gatehouse_client::login(base, username, password)?))
}

/// A signed-in session against one server.
#[derive(Clone, Debug)]
pub struct Session(gatehouse_client::Session);

impl Session {
    /// A session from a refresh token kept from an earlier one.
    #[must_use]
    pub fn resume(base: &str, refresh_token: &str) -> Self {
        gatehouse_client::trace_var("FORKLIFT_TRACE");
        Self(gatehouse_client::Session::resume(base, refresh_token))
    }

    /// The token to store: the new one after every refresh.
    #[must_use]
    pub fn refresh_token(&self) -> &str {
        self.0.refresh_token()
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

    /// An access token with life left (refreshed when there is none, or at about 80% of its life), for a host that
    /// sends the request itself.
    ///
    /// # Errors
    /// [`NetError::Unauthorized`] when the refresh token is no good: sign in again.
    pub fn fresh_token(&mut self) -> Result<String, NetError> {
        Ok(self.0.fresh_token()?)
    }

    fn get_with(
        &mut self,
        path: &str,
        query: &[(&str, String)],
        transfer: bool,
    ) -> Result<gatehouse_client::Response, NetError> {
        Ok(self.0.get(path, query, transfer)?)
    }

    /// `GET /files`: the storages this account may use, with owner, quota and use where the server says.
    ///
    /// # Errors
    /// [`NetError`].
    pub fn storages(&mut self) -> Result<Vec<Storage>, NetError> {
        let response = self.get_with("/warehouse/api/v1/files", &[], false)?;
        ok(response.status().as_u16())?;
        let value: Value = response.json().map_err(network)?;
        Ok(value
            .as_array()
            .map(|rows| rows.iter().filter_map(Storage::from_value).collect())
            .unwrap_or_default())
    }

    /// `GET /files/{storage}`: a page of what is stored under `prefix` (`n` entries after `last`).
    ///
    /// # Errors
    /// [`NetError::NoStorage`] for a `404`; the rest as [`NetError`].
    pub fn list(
        &mut self,
        storage: &str,
        prefix: &str,
        n: u32,
        last: Option<&str>,
    ) -> Result<Page, NetError> {
        let mut query = vec![("n", n.to_string())];
        if !prefix.is_empty() {
            query.push(("prefix", prefix.to_string()));
        }
        if let Some(last) = last {
            query.push(("last", last.to_string()));
        }
        let response = self.get_with(
            &format!("/warehouse/api/v1/files/{}", segment(storage)),
            &query,
            false,
        )?;
        ok(response.status().as_u16())?;
        let more = response.headers().get("link").is_some();
        let value: Value = response.json().map_err(network)?;
        let entries: Vec<Entry> = value
            .get("entries")
            .and_then(Value::as_array)
            .map(|rows| rows.iter().filter_map(Entry::from_value).collect())
            .unwrap_or_default();
        let next = if more {
            entries.last().map(|e| e.path.clone())
        } else {
            None
        };
        Ok(Page { entries, next })
    }

    /// Every file stored under `prefix` (all of it for an empty one), page after page. At most a million entries: a
    /// server that never ends its pages is not followed for ever.
    ///
    /// # Errors
    /// [`NetError::NoStorage`] for a `404`; the rest as [`NetError`].
    pub fn list_all(&mut self, storage: &str, prefix: &str) -> Result<Vec<Entry>, NetError> {
        let mut all = Vec::new();
        let mut last: Option<String> = None;
        for _ in 0..1000 {
            let page = self.list(storage, prefix, 1000, last.as_deref())?;
            let empty = page.entries.is_empty();
            all.extend(page.entries.into_iter().filter(|e| e.kind == "file"));
            match page.next {
                Some(next) if !empty && last.as_deref() != Some(next.as_str()) => last = Some(next),
                _ => break,
            }
        }
        Ok(all)
    }

    /// `GET /files/{storage}/sync?since=`: what changed after change number `since`, for finding out what the server
    /// already has.
    ///
    /// # Errors
    /// [`NetError::NoStorage`] for a `404`; the rest as [`NetError`].
    pub fn changes(&mut self, storage: &str, since: i64) -> Result<Vec<Change>, NetError> {
        let response = self.get_with(
            &format!("/warehouse/api/v1/files/{}/sync", segment(storage)),
            &[("since", since.to_string())],
            false,
        )?;
        ok(response.status().as_u16())?;
        let value: Value = response.json().map_err(network)?;
        Ok(value
            .as_array()
            .map(|rows| rows.iter().filter_map(Change::from_value).collect())
            .unwrap_or_default())
    }

    /// Sends one file whole (`PUT /files/{storage}/file?path=`). The host that can read the file itself uses this;
    /// Android's host streams from the media library instead, with [`upload_url`].
    ///
    /// # Errors
    /// [`NetError::TooLarge`], [`NetError::QuotaFull`], [`NetError::NoStorage`]; the rest as [`NetError`].
    pub fn put(&mut self, storage: &str, path: &str, body: Vec<u8>) -> Result<(), NetError> {
        let url = upload_url(self.0.base(), storage, path);
        let response = self.0.call("PUT", &url, true, |client, token| {
            client
                .put(&url)
                .bearer_auth(token)
                .header("content-type", "application/octet-stream")
                .body(body.clone())
                .send()
        })?;
        ok(response.status().as_u16())
    }

    /// Fetches one stored file whole (`GET /files/{storage}/file?path=`).
    ///
    /// # Errors
    /// [`NetError::NoStorage`] for a `404`; the rest as [`NetError`].
    pub fn get_file(&mut self, storage: &str, path: &str) -> Result<Vec<u8>, NetError> {
        let response = self.get_with(
            &format!("/warehouse/api/v1/files/{}/file", segment(storage)),
            &[("path", path.to_string())],
            true,
        )?;
        ok(response.status().as_u16())?;
        Ok(response.bytes().map_err(network)?.to_vec())
    }
}

/// A status as the result a caller can use.
fn ok(status: u16) -> Result<(), NetError> {
    match status {
        200..=299 => Ok(()),
        401 => Err(NetError::Unauthorized),
        403 => Err(NetError::Forbidden),
        404 => Err(NetError::NoStorage),
        413 => Err(NetError::TooLarge),
        507 => Err(NetError::QuotaFull),
        other => Err(NetError::Status(other)),
    }
}

/// One path segment, percent-encoded (a storage name is chosen by a person).
fn segment(s: &str) -> String {
    encode(s, false)
}

/// Percent-encodes everything but unreserved characters (and `/` when `keep_slash`).
fn encode(s: &str, keep_slash: bool) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(s.len());
    for byte in s.bytes() {
        if byte.is_ascii_alphanumeric()
            || matches!(byte, b'-' | b'.' | b'_' | b'~')
            || (keep_slash && byte == b'/')
        {
            out.push(char::from(byte));
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

/// Where a file is put: `{base}/warehouse/api/v1/files/{storage}/file?path={path}`, for a host that sends it itself.
#[must_use]
pub fn upload_url(base: &str, storage: &str, path: &str) -> String {
    format!(
        "{}/warehouse/api/v1/files/{}/file?path={}",
        base.trim().trim_end_matches('/'),
        segment(storage),
        encode(path, false)
    )
}

/// Sends `body` to `url` with `PUT` and the bearer token, whole, and says what the server answered. For a host that
/// reads the file itself and was handed the token ([`Session::fresh_token`]); a refusal (a status other than `2xx`) is
/// the answer, not an error.
///
/// # Errors
/// [`NetError::Network`] when the request cannot be made.
pub fn put_with_token(url: &str, token: &str, body: Vec<u8>) -> Result<u16, NetError> {
    let response = gatehouse_client::transfer_client()
        .put(url)
        .bearer_auth(token)
        .header("content-type", "application/octet-stream")
        .body(body)
        .send()
        .map_err(network)?;
    Ok(response.status().as_u16())
}

/// Fetches `url` with the bearer token: the status the server answered and, for a `2xx`, the body.
///
/// # Errors
/// [`NetError::Network`] when the request cannot be made or the body cannot be read.
pub fn get_with_token(url: &str, token: &str) -> Result<(u16, Vec<u8>), NetError> {
    let response = gatehouse_client::transfer_client()
        .get(url)
        .bearer_auth(token)
        .send()
        .map_err(network)?;
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        return Ok((status, Vec::new()));
    }
    Ok((status, response.bytes().map_err(network)?.to_vec()))
}

/// Where a stored file is fetched from: the same address as [`upload_url`], with `GET`.
#[must_use]
pub fn download_url(base: &str, storage: &str, path: &str) -> String {
    upload_url(base, storage, path)
}

/// A storage the account may use. Quota and use are there for a dynamic one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Storage {
    pub name: String,
    /// Who owns it: only a storage someone made (a dynamic one) has an owner; the ones the server's own configuration
    /// lists (static ones) have none.
    pub owner: Option<String>,
    pub quota_bytes: Option<u64>,
    pub used_bytes: Option<u64>,
}

impl Storage {
    fn from_value(v: &Value) -> Option<Self> {
        Some(Self {
            name: v.get("name")?.as_str()?.to_string(),
            owner: v.get("owner").and_then(Value::as_str).map(str::to_string),
            quota_bytes: v.get("quota_bytes").and_then(Value::as_u64),
            used_bytes: v.get("used_bytes").and_then(Value::as_u64),
        })
    }
}

/// One stored thing in a listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub path: String,
    /// `file` or `dir`, as the server says.
    pub kind: String,
    pub size: Option<u64>,
}

impl Entry {
    fn from_value(v: &Value) -> Option<Self> {
        Some(Self {
            name: v.get("name")?.as_str()?.to_string(),
            path: v.get("path")?.as_str()?.to_string(),
            kind: v
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or("file")
                .to_string(),
            size: v.get("size").and_then(Value::as_u64),
        })
    }
}

/// A page of a listing; `next` is what to pass as `last` for the following page, `None` on the last.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    pub entries: Vec<Entry>,
    pub next: Option<String>,
}

/// One change in a storage's feed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub id: i64,
    pub path: String,
    /// `put` or `delete`.
    pub op: String,
    pub size: Option<u64>,
}

impl Change {
    fn from_value(v: &Value) -> Option<Self> {
        Some(Self {
            id: v.get("id")?.as_i64()?,
            path: v.get("path")?.as_str()?.to_string(),
            op: v.get("op")?.as_str()?.to_string(),
            size: v.get("size").and_then(Value::as_u64),
        })
    }
}
