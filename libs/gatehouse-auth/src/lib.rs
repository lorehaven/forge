//! Application-token credentials for the command-line tools.
//!
//! A tool keeps `gatehouse_url` and a long-lived `app_token` in a git-ignored secret file: `.<tool>/secret.toml`
//! when the tool has a `.<tool>/` directory (where its configuration lives, too), else `.<tool>.secret.toml`. [`access_token`] exchanges it for a short-lived
//! bearer token on every run; [`login`] mints the app token in the first place.

use quench_core::{Context, Result, bail};
use quench_serde::{Deserialize, Serialize, Value};
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

/// The secret file name for a tool when it has no directory of its own, e.g. `.anvil.secret.toml`.
#[must_use]
pub fn secret_file_name(tool: &str) -> String {
    format!(".{tool}.secret.toml")
}

/// The secret file's path under the tool's directory, relative to the project: `.anvil/secret.toml`.
#[must_use]
pub fn secret_dir_file(tool: &str) -> String {
    format!(".{tool}/secret.toml")
}

/// Looks for the tool's secret file in `start` and each parent directory: in each, `.<tool>/secret.toml` first, then
/// `.<tool>.secret.toml`.
#[must_use]
pub fn find_secret_file(tool: &str, start: &Path) -> Option<PathBuf> {
    let (inside, beside) = (secret_dir_file(tool), secret_file_name(tool));
    start
        .ancestors()
        .flat_map(|dir| [dir.join(&inside), dir.join(&beside)])
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

/// Writes the tool's secret file into `dir`, readable by its owner only, and makes sure git ignores it.
///
/// Returns the file's path: `.<tool>/secret.toml` when `dir` has the tool's directory, else `.<tool>.secret.toml`. A
/// secret file of the other kind that this replaces is removed, so one login leaves one token behind.
pub fn save(tool: &str, dir: &Path, credentials: &Credentials) -> Result<PathBuf> {
    let text = quench_serde::toml::to_string(credentials).context("serialising credentials")?;
    let (inside, beside) = (
        dir.join(secret_dir_file(tool)),
        dir.join(secret_file_name(tool)),
    );
    let (path, ignored, old) = if dir.join(format!(".{tool}")).is_dir() {
        (inside, secret_dir_file(tool), beside)
    } else {
        (beside, secret_file_name(tool), inside)
    };
    // Whatever else the file keeps (see [`set_extra`]) survives a new login.
    let text = match fs::read_to_string(&path)
        .ok()
        .and_then(|old| quench_serde::toml::parse(&old).ok())
    {
        Some(Value::Object(mut kept)) => {
            if let Value::Object(fresh) =
                quench_serde::toml::parse(&text).context("reading back")?
            {
                for (key, value) in fresh.iter() {
                    kept.insert(key.clone(), value.clone());
                }
            }
            quench_serde::toml::to_string(&Value::Object(kept))
                .context("serialising credentials")?
        }
        _ => text,
    };
    write_private(&path, &text)?;
    ensure_ignored(dir, &ignored)?;
    if old.is_file() {
        fs::remove_file(&old).with_context(|| format!("removing the old {}", old.display()))?;
    }
    Ok(path)
}

/// A value the tool keeps in its secret file besides the credentials, `[table] key = "..."` (the Android keystore's
/// passwords, for one). `None` when there is no secret file or no such value.
pub fn extra(tool: &str, start: &Path, table: &str, key: &str) -> Result<Option<String>> {
    let Some(path) = find_secret_file(tool, start) else {
        return Ok(None);
    };
    let text = fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let value =
        quench_serde::toml::parse(&text).with_context(|| format!("parsing {}", path.display()))?;
    Ok(value
        .get(table)
        .and_then(|t| t.get(key))
        .and_then(Value::as_str)
        .map(str::to_string))
}

/// Stores `[table] key = "value"` in the tool's existing secret file, leaving the rest of it as it was.
///
/// # Errors
/// There is no secret file (`login` makes it), or it cannot be read, parsed or written.
pub fn set_extra(tool: &str, start: &Path, table: &str, key: &str, value: &str) -> Result<PathBuf> {
    let Some(path) = find_secret_file(tool, start) else {
        bail!("no secret file: run `{tool} login` first");
    };
    let text = fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let Value::Object(mut all) =
        quench_serde::toml::parse(&text).with_context(|| format!("parsing {}", path.display()))?
    else {
        bail!("{} is not a TOML table", path.display());
    };
    let mut inner = match all.get(table) {
        Some(Value::Object(inner)) => inner.clone(),
        _ => quench_serde::Map::new(),
    };
    inner.insert(key.to_string(), Value::from(value));
    all.insert(table.to_string(), Value::Object(inner));
    let text = quench_serde::toml::to_string(&Value::Object(all)).context("serialising")?;
    write_private(&path, &text)?;
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
        .any(|line| matches!(line.trim(), l if l == pattern || l == ".*.secret.toml" || l == ".*/secret.toml"))
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
    quench_cli::term::prompt_password(prompt).context("reading the password")
}

/// `Bearer <token>`, the form Cargo registries and Warehouse expect verbatim.
#[must_use]
pub fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}
