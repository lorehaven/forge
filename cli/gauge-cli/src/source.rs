//! Where the numbers come from: sign in to Gatehouse once, then ask Gauge.

use crate::config::FileConfig;
use crate::state::Data;
use gatehouse_client::Session;
use quench_core::{Context, Result, bail};
use quench_serde::DeserializeOwned;
use std::fmt::Write;
use std::path::PathBuf;

/// How the session is had.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Auth {
    /// Sign in with an account's password (the password is used once and not kept).
    Password { username: String, password: String },
    /// Resume a session from the refresh token the config file holds.
    Token(String),
}

/// Where Gauge is and who asks. The server is the root both services share (`https://ennor.ddns.net`), since the
/// sign-in is Gatehouse's, at `{server}/gatehouse`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub server: String,
    /// Gauge's path under the server.
    pub path: String,
    pub auth: Auth,
    /// The config file that holds the token, where a renewed one is kept.
    pub file: Option<PathBuf>,
}

/// A flag, else the environment, trimmed; blank counts as unset.
fn pick(flag: Option<String>, key: &str, env: &impl Fn(&str) -> Option<String>) -> Option<String> {
    flag.or_else(|| env(key))
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// The server's address, from a flag, `GAUGE_URL` or the file.
///
/// # Errors
/// Nothing gives it.
pub fn server_of(
    url: Option<String>,
    file: &FileConfig,
    env: &impl Fn(&str) -> Option<String>,
) -> Result<String> {
    let server = pick(url, "GAUGE_URL", env)
        .or_else(|| file.url.clone().filter(|u| !u.trim().is_empty()))
        .context(
            "the server's address is not set: run `gauge login --url https://ennor.ddns.net`, \
             or pass --url, or set GAUGE_URL",
        )?;
    Ok(server.trim().trim_end_matches('/').to_string())
}

impl Config {
    /// Flags win over the environment (`GAUGE_URL`, `GAUGE_PATH`, `GAUGE_USERNAME`, `GAUGE_PASSWORD`,
    /// `GAUGE_TOKEN`), which wins over the config file. A password sign-in beats a stored token.
    ///
    /// # Errors
    /// What is missing, and how to give it.
    pub fn resolve(
        flags: Flags,
        file: &FileConfig,
        file_path: Option<PathBuf>,
        env: impl Fn(&str) -> Option<String>,
    ) -> Result<Self> {
        let server = server_of(flags.url, file, &env)?;
        let path = pick(flags.path, "GAUGE_PATH", &env)
            .or_else(|| file.path.clone().filter(|p| !p.trim().is_empty()))
            .unwrap_or_else(|| "/gauge".to_string());
        let username = pick(flags.username, "GAUGE_USERNAME", &env);
        let password = pick(flags.password, "GAUGE_PASSWORD", &env);
        let token = pick(None, "GAUGE_TOKEN", &env)
            .or_else(|| file.token.clone().filter(|t| !t.trim().is_empty()));
        let auth = match (username, password, token) {
            (Some(username), Some(password), _) => Auth::Password { username, password },
            (Some(_), None, _) => bail!("no password: pass --password or set GAUGE_PASSWORD"),
            (None, _, Some(token)) => Auth::Token(token),
            (None, _, None) => {
                bail!("not signed in: run `gauge login`, or set GAUGE_USERNAME and GAUGE_PASSWORD")
            }
        };
        Ok(Self {
            server,
            path: format!("/{}", path.trim_matches('/'))
                .trim_end_matches('/')
                .to_string(),
            auth,
            file: file_path,
        })
    }
}

/// The command-line flags that feed [`Config::resolve`].
#[derive(Clone, Debug, Default)]
pub struct Flags {
    pub url: Option<String>,
    pub path: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
}

/// A signed-in session with Gauge. Blocking: it lives on a worker thread.
#[derive(Debug)]
pub struct Source {
    session: Session,
    path: String,
}

impl Source {
    /// Signs in, or resumes the stored session (renewing it once now, so a token that no longer works is an error
    /// line before the screen opens). A renewed token is written back to the config file as soon as Gatehouse gives
    /// it: it replaces the old one, which is spent.
    ///
    /// # Errors
    /// Gatehouse's refusal or an unreachable server, in words.
    pub fn connect(config: &Config) -> Result<Self> {
        let session = match &config.auth {
            Auth::Password { username, password } => {
                gatehouse_client::login(&config.server, username, password)
                    .map_err(|e| quench_core::err!("{e}"))?
            }
            Auth::Token(token) => {
                let mut session = Session::resume(&config.server, token);
                if let Some(file) = config.file.clone() {
                    session = session.keeping_tokens(move |new| {
                        let _ = FileConfig::store_token(&file, new);
                    });
                }
                session.refresh().map_err(|e| match e {
                    gatehouse_client::Error::Unauthorized => quench_core::err!(
                        "the stored session has ended: run `gauge login` to sign in again"
                    ),
                    other => quench_core::err!("{other}"),
                })?;
                session
            }
        };
        Ok(Self {
            session,
            path: config.path.clone(),
        })
    }

    fn get<T: DeserializeOwned>(&mut self, what: &str) -> Result<Option<T>> {
        let path = format!("{}/api/v1/{what}", self.path);
        let response = self
            .session
            .get(&path, &[], false)
            .map_err(|e| quench_core::err!("{e}"))?;
        let status = response.status().as_u16();
        match status {
            200 => {
                let body = response.text().context("could not read the answer")?;
                let value = quench_serde::json::from_str(&body).with_context(|| {
                    format!("{what}: the service answered something unexpected")
                })?;
                Ok(Some(value))
            }
            // No stored history yet.
            503 => Ok(None),
            403 => bail!("this account may not see Gauge: ask for access in gatehouse"),
            404 => bail!("Gauge is not at {path}: check --url and --path"),
            other => bail!("{what}: the service answered {other}"),
        }
    }

    /// One round of fetches; `open` is the host whose page is open, which also gets its history. The host list is what the screen needs; the forecast and the alert log are extras, so
    /// a service without history still shows its hosts.
    ///
    /// # Errors
    /// The host list could not be had.
    pub fn fetch(&mut self, open: Option<&str>) -> Result<Data> {
        let hosts = self.get("hosts")?.unwrap_or_default();
        let detail = open.and_then(|name| self.get(&host_path(name)).ok().flatten());
        let forecast = self.get("forecast").ok().flatten().unwrap_or_default();
        let alerts = self
            .get("alerts/history")
            .ok()
            .flatten()
            .unwrap_or_default();
        Ok(Data {
            hosts,
            forecast,
            alerts,
            detail,
        })
    }
}

/// `hosts/{name}`, the name percent-encoded.
#[must_use]
pub fn host_path(name: &str) -> String {
    let mut path = String::from("hosts/");
    for byte in name.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            path.push(char::from(byte));
        } else {
            let _ = write!(path, "%{byte:02X}");
        }
    }
    path
}
