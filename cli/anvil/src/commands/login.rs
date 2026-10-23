use crate::config::Config;
use gatehouse_auth::{Credentials, NewToken};
use quench_core::{Context, Result, bail};
use std::io::{self, Write as _};
use std::path::Path;

const TOOL: &str = "anvil";

/// Mints an application token for this machine and stores it in `.anvil.secret.toml`.
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
            "no Gatehouse URL: pass --gatehouse-url, or set `gatehouse_url` under [release] in .anvil.toml",
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
