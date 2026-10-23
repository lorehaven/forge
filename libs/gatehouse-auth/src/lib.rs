//! Application-token credentials for the command-line tools.
//!
//! A tool keeps `gatehouse_url` and a long-lived `app_token` in a git-ignored
//! `.<tool>.secret.toml`. [`access_token`] exchanges it for a short-lived
//! bearer token on every run; [`login`] mints the app token in the first place.

use quench_core::{Context, Result, bail};
use quench_serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// What a tool keeps in its secret file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Credentials {
    /// Gatehouse including its base path, e.g. `https://host/gatehouse`.
    pub gatehouse_url: String,
    pub app_token: String,
}

/// The secret file name for a tool, e.g. `.anvil.secret.toml`.
#[must_use]
pub fn secret_file_name(tool: &str) -> String {
    format!(".{tool}.secret.toml")
}

/// Looks for the tool's secret file in `start` and each parent directory.
#[must_use]
pub fn find_secret_file(tool: &str, start: &Path) -> Option<PathBuf> {
    let name = secret_file_name(tool);
    start
        .ancestors()
        .map(|dir| dir.join(&name))
        .find(|candidate| candidate.is_file())
}

/// The tool's stored credentials, or `None` when it has no secret file.
pub fn load(tool: &str, start: &Path) -> Result<Option<Credentials>> {
    let Some(path) = find_secret_file(tool, start) else {
        return Ok(None);
    };
    let text = fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let credentials = quench_serde::toml::from_str(&text)
        .with_context(|| format!("parsing {}", path.display()))?;
    Ok(Some(credentials))
}

/// Writes the tool's secret file into `dir`, readable by its owner only, and
/// makes sure git ignores it. Returns the file's path.
pub fn save(tool: &str, dir: &Path, credentials: &Credentials) -> Result<PathBuf> {
    let path = dir.join(secret_file_name(tool));
    let text = quench_serde::toml::to_string(credentials).context("serialising credentials")?;
    write_private(&path, &text)?;
    ensure_ignored(dir, &secret_file_name(tool))?;
    Ok(path)
}

#[cfg(unix)]
fn write_private(path: &Path, text: &str) -> Result<()> {
    use std::os::unix::fs::OpenOptionsExt as _;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .with_context(|| format!("writing {}", path.display()))?;
    file.write_all(text.as_bytes())
        .with_context(|| format!("writing {}", path.display()))
}

#[cfg(not(unix))]
fn write_private(path: &Path, text: &str) -> Result<()> {
    fs::write(path, text).with_context(|| format!("writing {}", path.display()))
}

/// Appends `pattern` to `dir/.gitignore` unless a line already says so.
fn ensure_ignored(dir: &Path, pattern: &str) -> Result<()> {
    let path = dir.join(".gitignore");
    let existing = fs::read_to_string(&path).unwrap_or_default();
    if existing
        .lines()
        .any(|line| matches!(line.trim(), l if l == pattern || l == ".*.secret.toml"))
    {
        return Ok(());
    }
    let separator = if existing.is_empty() || existing.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    fs::write(&path, format!("{existing}{separator}{pattern}\n"))
        .with_context(|| format!("updating {}", path.display()))
}

fn client() -> Result<quench_client::blocking::Client> {
    quench_client::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .context("building the HTTP client")
}

#[derive(Deserialize)]
struct AccessToken {
    access_token: String,
}

/// Exchanges the stored app token for a short-lived access token.
pub fn access_token(credentials: &Credentials) -> Result<String> {
    let url = format!(
        "{}/api/v1/token",
        credentials.gatehouse_url.trim_end_matches('/')
    );
    let client = client()?;
    let response = client
        .post(&url)
        .form(&[
            ("grant_type", "app_token"),
            ("app_token", credentials.app_token.as_str()),
        ])
        .send()
        .with_context(|| format!("reaching Gatehouse at {url}"))?;
    match response.status().as_u16() {
        200 => Ok(response
            .json::<AccessToken>()
            .context("reading Gatehouse's token response")?
            .access_token),
        401 => bail!(
            "Gatehouse rejected the application token (revoked, expired or unknown) - \
             run the tool's `login` again"
        ),
        status => bail!("Gatehouse answered {status} to the token exchange"),
    }
}

/// How the app token to mint should look.
#[derive(Debug, Clone, Default)]
pub struct NewToken {
    pub name: String,
    /// `service:action` entries; empty is the owner's full access.
    pub scope: Vec<String>,
    pub expires_in_days: Option<i64>,
}

#[derive(Deserialize)]
struct Created {
    secret: String,
}

/// Signs in with a username and password, mints an application token and
/// returns it. The password is used for this one call and never stored.
pub fn login(
    gatehouse_url: &str,
    username: &str,
    password: &str,
    new: &NewToken,
) -> Result<Credentials> {
    let base = gatehouse_url.trim_end_matches('/');
    let http = client()?;

    let session = http
        .post(format!("{base}/api/v1/auth/login"))
        .json(&quench_serde::json!({ "username": username, "password": password }))
        .send()
        .with_context(|| format!("reaching Gatehouse at {base}"))?;
    if !session.status().is_success() {
        bail!("Gatehouse refused the login ({})", session.status());
    }
    let session: AccessToken = session
        .json()
        .context("reading Gatehouse's login response")?;

    let created = http
        .post(format!("{base}/api/v1/me/tokens"))
        .bearer_auth(&session.access_token)
        .json(&quench_serde::json!({
            "name": new.name,
            "scope": new.scope,
            "expires_in_days": new.expires_in_days,
        }))
        .send()
        .context("asking Gatehouse for an application token")?;
    let status = created.status();
    if !status.is_success() {
        let detail = created.text().unwrap_or_default();
        bail!("Gatehouse refused to create the token ({status}): {detail}");
    }
    let created: Created = created.json().context("reading the created token")?;
    Ok(Credentials {
        gatehouse_url: base.to_string(),
        app_token: created.secret,
    })
}

/// Reads a password from the terminal without echoing it.
pub fn prompt_password(prompt: &str) -> Result<String> {
    rpassword::prompt_password(prompt).context("reading the password")
}

/// `Bearer <token>`, the form Cargo registries and Warehouse expect verbatim.
#[must_use]
pub fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}
