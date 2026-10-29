//! The session every Forklift-style client of gatehouse needs, written once: sign in, keep the rotating refresh token
//! safe, hand out an access token with life left, and make a request with the bearer (renewing the token and trying
//! again once when the server refuses it).
//!
//! Plain blocking calls (run them on a worker thread). The password is used once, by [`login`]; the refresh token
//! rotates, so a new one is handed to the caller to keep **before** the request that needed it
//! ([`Session::keeping_tokens`]), and two programs of one user share a session by looking at what the other kept
//! ([`Session::following`]).

use std::fmt;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

pub use quench_client::blocking::{Client, Response};
use quench_serde::{Deserialize, Value, json};

/// What went wrong, in the terms a screen shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
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
}

impl fmt::Display for Error {
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
            Self::EmailUnverified => {
                f.write_str("confirm the email address of this account first (gatehouse sent a link)")
            }
        }
    }
}

impl std::error::Error for Error {}

#[must_use]
pub fn network(e: impl fmt::Display) -> Error {
    Error::Network(e.to_string())
}

/// The tokens gatehouse gives.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct Tokens {
    access_token: String,
    refresh_token: String,
    #[serde(default)]
    expires_in: i64,
}

/// How long a call to a service may take (the whole call), and how long a file may take to arrive.
///
/// A call that never answers would hold the worker thread for ever, and with it everything waiting behind it: the
/// loader spins and nothing else is asked.
static TIMEOUTS: OnceLock<(Duration, Duration)> = OnceLock::new();

/// Sets the timeouts, once and before the first call (the first call fixes them). A test uses short ones.
pub fn set_timeouts(call: Duration, transfer: Duration) {
    let _ = TIMEOUTS.set((call, transfer));
}

fn timeouts() -> (Duration, Duration) {
    *TIMEOUTS.get_or_init(|| (Duration::from_secs(20), Duration::from_secs(15 * 60)))
}

/// The client for calls: made once, because a client owns the threads its requests run on.
#[must_use]
pub fn client() -> &'static Client {
    static CLIENT: OnceLock<Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        Client::builder()
            .timeout(timeouts().0)
            .build()
            .expect("the default client configuration is valid")
    })
}

/// The client for files, which are allowed their time.
#[must_use]
pub fn transfer_client() -> &'static Client {
    static CLIENT: OnceLock<Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        Client::builder()
            .timeout(timeouts().1)
            .build()
            .expect("the default client configuration is valid")
    })
}

/// What is told the new refresh token, the moment a refresh gives one.
type OnRefresh = std::sync::Arc<dyn Fn(&str) + Send + Sync>;

/// Where the newest refresh token kept for this session is, whoever kept it.
type LatestToken = std::sync::Arc<dyn Fn() -> Option<String> + Send + Sync>;

/// A signed-in session against one server.
#[derive(Clone)]
pub struct Session {
    base: String,
    access: String,
    refresh: String,
    expires_in: i64,
    /// When the access token was given; `None` until there is one.
    obtained: Option<Instant>,
    on_refresh: Option<OnRefresh>,
    latest: Option<LatestToken>,
}

impl fmt::Debug for Session {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The tokens stay out of any log.
        f.debug_struct("Session")
            .field("base", &self.base)
            .field("expires_in", &self.expires_in)
            .finish_non_exhaustive()
    }
}

fn base_of(url: &str) -> String {
    url.trim().trim_end_matches('/').to_string()
}

/// The environment variable that turns the trace on (the app's own, set by [`trace_var`]).
static TRACE_VAR: OnceLock<&'static str> = OnceLock::new();

/// Names the environment variable that, when set, makes every request say what was asked and what came back on
/// standard error (the method, the path without its query, the status; never a token, a password or a body). Call it
/// once, before the first request; the first call wins.
pub fn trace_var(name: &'static str) {
    let _ = TRACE_VAR.set(name);
}

/// Says `method path -> outcome` when the trace is on.
pub fn trace(method: &str, url: &str, outcome: &str) {
    if let Some(name) = TRACE_VAR.get()
        && std::env::var_os(name).is_some()
    {
        let path = url.split('?').next().unwrap_or(url);
        eprintln!("gatehouse-client: {method} {path} -> {outcome}");
    }
}

fn post_json(client: &Client, url: &str, body: &Value) -> Result<(u16, String), Error> {
    let response = client.post(url).json(body).send().map_err(|e| {
        trace("POST", url, &format!("failed: {e}"));
        network(e)
    })?;
    let status = response.status().as_u16();
    trace("POST", url, &status.to_string());
    Ok((status, response.text().map_err(network)?))
}

fn tokens(base: &str, status: u16, text: &str) -> Result<Session, Error> {
    match status {
        200 => {
            let t: Tokens = json::from_str(text).map_err(network)?;
            Ok(Session {
                base: base.to_string(),
                access: t.access_token,
                refresh: t.refresh_token,
                expires_in: t.expires_in,
                obtained: Some(Instant::now()),
                on_refresh: None,
                latest: None,
            })
        }
        401 | 403 if text.contains("mfa_required") => Err(Error::MfaRequired),
        400..=403 if text.contains("mfa_required") => Err(Error::MfaRequired),
        // Gatehouse says why in the body (`{"error": "account_locked"}`): a locked account is not a wrong password.
        400 | 401 | 403 if text.contains("account_locked") => Err(Error::AccountLocked),
        400 | 401 | 403 if text.contains("account_disabled") => Err(Error::AccountDisabled),
        400 | 401 | 403 if text.contains("email_unverified") => Err(Error::EmailUnverified),
        400 | 401 | 403 => Err(Error::Unauthorized),
        other => Err(Error::Status(other)),
    }
}

/// Signs in. The password is not kept.
///
/// # Errors
/// [`Error`].
pub fn login(base: &str, username: &str, password: &str) -> Result<Session, Error> {
    let base = base_of(base);
    let body = json!({ "username": username, "password": password });
    let (status, text) = post_json(
        client(),
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
            obtained: None,
            on_refresh: None,
            latest: None,
        }
    }

    /// The token to store: the new one after every refresh.
    #[must_use]
    pub fn refresh_token(&self) -> &str {
        &self.refresh
    }

    /// An access token with life left (refreshed when there is none, or at about 80% of its life), for a host that
    /// sends the request itself.
    ///
    /// # Errors
    /// [`Error::Unauthorized`] when the refresh token is no good: sign in again.
    pub fn fresh_token(&mut self) -> Result<String, Error> {
        let stale = self.obtained.is_none_or(|at| {
            let life = u64::try_from(self.expires_in).unwrap_or(0);
            at.elapsed() >= Duration::from_secs(life * 4 / 5)
        });
        if self.access.is_empty() || stale {
            self.refresh()?;
        }
        Ok(self.access.clone())
    }

    /// Seconds the access token lives (refresh at about 80% of it).
    #[must_use]
    pub const fn expires_in(&self) -> i64 {
        self.expires_in
    }

    /// Calls `store` with every new refresh token, as soon as it is given - before the request that needed it is
    /// answered. A refresh spends the old token, so one that is not kept (the request failed, the app was closed
    /// meanwhile) leaves the next start with a token that no longer works, and the user signed out.
    #[must_use]
    pub fn keeping_tokens(mut self, store: impl Fn(&str) + Send + Sync + 'static) -> Self {
        self.on_refresh = Some(std::sync::Arc::new(store));
        self
    }

    /// Asks `latest` for the newest refresh token kept by anyone before a refresh, and again when a refresh is
    /// refused. Two programs of one user (the app, and the check that runs in the background) share one session: a
    /// refresh by either spends the token the other holds, and without this the second would find its token refused
    /// and the user would be signed out.
    #[must_use]
    pub fn following(
        mut self,
        latest: impl Fn() -> Option<String> + Send + Sync + 'static,
    ) -> Self {
        self.latest = Some(std::sync::Arc::new(latest));
        self
    }

    /// A refresh token someone kept that is not the one this session holds.
    fn newer_token(&self) -> Option<String> {
        self.latest
            .as_ref()
            .and_then(|latest| latest())
            .filter(|token| !token.is_empty() && *token != self.refresh)
    }

    /// Gets a new access token (and a new refresh token: the old one is spent).
    ///
    /// # Errors
    /// [`Error::Unauthorized`] when the refresh token is no good: sign in again.
    pub fn refresh(&mut self) -> Result<(), Error> {
        if let Some(newer) = self.newer_token() {
            self.refresh = newer;
        }
        match self.exchange() {
            Err(Error::Unauthorized) => {
                // Spent a moment ago by the other program? Then the one it kept is the one to use.
                if let Some(newer) = self.newer_token() {
                    self.refresh = newer;
                    self.exchange()
                } else {
                    Err(Error::Unauthorized)
                }
            }
            other => other,
        }
    }

    fn exchange(&mut self) -> Result<(), Error> {
        let body = json!({ "refresh_token": self.refresh });
        let (status, text) = post_json(
            client(),
            &format!("{}/gatehouse/api/v1/auth/refresh", self.base),
            &body,
        )?;
        // A refusal leaves the session as it was, hooks and all.
        let fresh = tokens(&self.base, status, &text)?;
        let (keep, latest) = (self.on_refresh.take(), self.latest.take());
        *self = fresh;
        self.on_refresh = keep;
        self.latest = latest;
        if let Some(keep) = &self.on_refresh {
            keep(&self.refresh);
        }
        Ok(())
    }

    /// The server this session belongs to (no trailing slash).
    #[must_use]
    pub fn base(&self) -> &str {
        &self.base
    }

    /// Makes a request with the bearer token: `send` is given the client to use (the one for calls, which must
    /// answer in time, or with `transfer` the one for files, which are allowed theirs) and the access token. With no
    /// token yet one is fetched first; a `401` renews the token and sends once more. `method` and `path` are for the
    /// trace only.
    ///
    /// # Errors
    /// [`Error::Network`] when the request cannot be made; [`Error::Unauthorized`] when the session is no good.
    pub fn call(
        &mut self,
        method: &str,
        path: &str,
        transfer: bool,
        send: impl Fn(&Client, &str) -> Result<Response, quench_client::transport::Error>,
    ) -> Result<Response, Error> {
        for attempt in 0..2 {
            if self.access.is_empty() && attempt == 0 {
                self.refresh()?;
            }
            let response = send(
                if transfer {
                    transfer_client()
                } else {
                    client()
                },
                &self.access,
            )
            .map_err(|e| {
                trace(method, path, &format!("failed: {e}"));
                network(e)
            })?;
            trace(method, path, &response.status().as_u16().to_string());
            if response.status().as_u16() == 401 && attempt == 0 {
                self.refresh()?;
                continue;
            }
            return Ok(response);
        }
        Err(Error::Unauthorized)
    }

    /// A `GET` of `path` (under the server, with the query) with the bearer token.
    ///
    /// # Errors
    /// As [`call`](Self::call).
    pub fn get(
        &mut self,
        path: &str,
        query: &[(&str, String)],
        transfer: bool,
    ) -> Result<Response, Error> {
        let url = format!("{}{path}", self.base);
        self.call("GET", path, transfer, |client, token| {
            client.get(&url).query(query).bearer_auth(token).send()
        })
    }
}
