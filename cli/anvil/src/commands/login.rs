use crate::config::Config;
use gatehouse_auth::{Credentials, NewToken};
use quench_core::{Context, Result, bail};
use std::io::{self, Write as _};
use std::path::Path;

const TOOL: &str = "anvil";

/// Mints an application token for this machine and stores it in `.anvil/secret.toml` (or `.anvil.secret.toml` when there is no `.anvil/`).
pub fn login(
    config: &Config,
    gatehouse_url: Option<String>,
    username: Option<String>,
    name: Option<String>,
    scope: Vec<String>,
    days: Option<i64>,
) -> Result<()> {
    let dir = std::env::current_dir().context("finding the working directory")?;
    let stored = gatehouse_auth::load(TOOL, &dir)?.map(|stored| stored.gatehouse_url);
    let url = gatehouse_url
        .or_else(|| config.release.gatehouse_url.clone())
        .or(stored)
        .context(
            "no Gatehouse URL: pass --gatehouse-url, or set `gatehouse_url` under [release] in .anvil.toml or .anvil/",
        )?;
    let username = match username {
        Some(username) => username,
        None => prompt("Gatehouse username: ")?,
    };
    let password = gatehouse_auth::prompt_password("Password: ")?;
    let new = NewToken {
        name: name.unwrap_or_else(|| default_token_name(&hostname())),
        scope,
        expires_in_days: days,
    };

    let credentials = gatehouse_auth::login(&url, &username, &password, &new)?;
    let path = gatehouse_auth::save(TOOL, &dir, &credentials)?;
    println!("Stored an application token in {}", path.display());
    println!("It is git-ignored; revoke it from Gatehouse if the machine is lost.");
    Ok(())
}

/// The table of the secret file that keeps the Android signing passwords, and the variables the build reads them from.
const ANDROID: &str = "android";
const PASSWORDS: [(&str, &str); 2] = [
    ("keystore_password", "QUENCH_KEYSTORE_PASSWORD"),
    ("key_password", "QUENCH_KEY_PASSWORD"),
];

/// Stores the release keystore's password (and the key's own, when it has one) in the secret file, which is git-ignored
/// and readable by its owner only.
pub fn keystore_password(separate_key_password: bool) -> Result<()> {
    let dir = std::env::current_dir().context("finding the working directory")?;
    let password = gatehouse_auth::prompt_password("Keystore password: ")?;
    if password.is_empty() {
        bail!("the keystore password is required");
    }
    let path = gatehouse_auth::set_extra(TOOL, &dir, ANDROID, PASSWORDS[0].0, &password)?;
    if separate_key_password {
        let key = gatehouse_auth::prompt_password("Key password: ")?;
        if key.is_empty() {
            bail!("the key password is required (leave out --key when it is the keystore's)");
        }
        gatehouse_auth::set_extra(TOOL, &dir, ANDROID, PASSWORDS[1].0, &key)?;
    }
    println!("Stored the keystore password in {}", path.display());
    Ok(())
}

/// The signing passwords for a build run from `dir`: what the secret file keeps, for every variable the environment does
/// not already set (the environment wins, as it does for the registry token).
pub fn keystore_env(
    dir: &Path,
    env_has: impl Fn(&str) -> bool,
) -> Result<Vec<(&'static str, String)>> {
    let mut found = Vec::new();
    for (key, var) in PASSWORDS {
        if env_has(var) {
            continue;
        }
        if let Some(value) = gatehouse_auth::extra(TOOL, dir, ANDROID, key)? {
            found.push((var, value));
        }
    }
    Ok(found)
}

/// The environment variable Cargo reads a registry's token from.
#[must_use]
pub fn registry_token_var(registry: &str) -> String {
    format!(
        "CARGO_REGISTRIES_{}_TOKEN",
        registry.to_uppercase().replace('-', "_")
    )
}

/// The token variable to set on a `cargo` child process publishing to
/// `registry`, from the stored application token.
///
/// `None` when the user already provides a token for the registry through the
/// environment, or has no secret file - Cargo then falls back to whatever it
/// has configured, as before.
pub fn registry_token_env(
    registry: &str,
    dir: &Path,
    env_has: impl Fn(&str) -> bool,
) -> Result<Option<(String, String)>> {
    let var = registry_token_var(registry);
    if env_has(&var) {
        return Ok(None);
    }
    let Some(credentials) = gatehouse_auth::load(TOOL, dir)? else {
        return Ok(None);
    };
    let token = exchange(&credentials)?;
    Ok(Some((var, gatehouse_auth::bearer(&token))))
}

/// Stored credentials, or - for a CI job with no secret file - `ANVIL_APP_TOKEN` and
/// `ANVIL_GATEHOUSE_URL` from the environment.
pub fn credentials_from(
    dir: &Path,
    env: impl Fn(&str) -> Option<String>,
) -> Result<Option<Credentials>> {
    let from_env = |name: &str| {
        env(name)
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    };
    if let (Some(app_token), Some(gatehouse_url)) =
        (from_env("ANVIL_APP_TOKEN"), from_env("ANVIL_GATEHOUSE_URL"))
    {
        return Ok(Some(Credentials {
            gatehouse_url,
            app_token,
        }));
    }
    gatehouse_auth::load(TOOL, dir)
}

/// The `Authorization` value Cargo sends to a private registry, as a Cargo credential provider
/// prints it: `Bearer <short-lived access token>`.
///
/// # Errors
///
/// When there are no credentials, or Gatehouse refuses or cannot be reached.
pub fn registry_token(dir: &Path, env: impl Fn(&str) -> Option<String>) -> Result<String> {
    let credentials = credentials_from(dir, env)?.context(
        "no credentials: run `anvil login`, or set ANVIL_APP_TOKEN and ANVIL_GATEHOUSE_URL",
    )?;
    Ok(gatehouse_auth::bearer(&exchange(&credentials)?))
}

fn exchange(credentials: &Credentials) -> Result<String> {
    gatehouse_auth::access_token(credentials)
        .context("could not get an access token from the stored application token")
}

#[must_use]
pub fn default_token_name(host: &str) -> String {
    format!("anvil on {host}")
}

fn hostname() -> String {
    std::env::var("HOSTNAME")
        .ok()
        .filter(|host| !host.is_empty())
        .unwrap_or_else(|| "this machine".to_string())
}

fn prompt(label: &str) -> Result<String> {
    print!("{label}");
    io::stdout().flush().context("writing the prompt")?;
    let mut line = String::new();
    io::stdin()
        .read_line(&mut line)
        .context("reading your answer")?;
    let answer = line.trim().to_string();
    if answer.is_empty() {
        bail!("{label}is required");
    }
    Ok(answer)
}
