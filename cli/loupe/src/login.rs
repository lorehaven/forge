//! `loupe login`: an application token for reading released books.
//!
//! The token is minted by Gatehouse and kept by gatehouse-auth, as `anvil login` and `riveter login` do. loupe is a reader's tool rather than a project's, so
//! the token goes into its config directory (`~/.config/loupe/.loupe.secret.toml`, readable by its owner only),
//! where every later run finds it; a `.loupe.secret.toml` in or above the working directory wins over it.

use crate::cli::LoginArgs;
use crate::released::TOOL;
use gatehouse_auth::NewToken;
use std::path::{Path, PathBuf};

/// The grant a token minted by `loupe login` asks for unless `--scope` says otherwise: reading is all loupe does.
pub const DEFAULT_SCOPE: &str = "warehouse:read";

/// The name of a token minted on `host`.
#[must_use]
pub fn default_token_name(host: &str) -> String {
    format!("loupe on {host}")
}

/// The token to ask for.
#[must_use]
pub fn new_token(args: &LoginArgs, host: &str) -> NewToken {
    NewToken {
        name: args
            .name
            .clone()
            .unwrap_or_else(|| default_token_name(host)),
        scope: if args.scope.is_empty() {
            vec![DEFAULT_SCOPE.to_string()]
        } else {
            args.scope.clone()
        },
        expires_in_days: args.days,
    }
}

/// Signs in (the password read without echo) and stores the token in `config_dir`.
///
/// # Errors
/// No Gatehouse URL, Gatehouse refuses, or the token cannot be stored.
pub fn login(
    args: &LoginArgs,
    url: Option<String>,
    config_dir: &Path,
    ask: &mut dyn FnMut(&str) -> Result<String, String>,
) -> Result<PathBuf, String> {
    let stored = gatehouse_auth::load(TOOL, config_dir)
        .map_err(|e| e.to_string())?
        .map(|c| c.gatehouse_url);
    let url = url.or(stored).ok_or(
        "no Gatehouse URL: pass --gatehouse-url, or set LOUPE_GATEHOUSE_URL, or `gatehouse_url` in \
         ~/.config/loupe/config.toml",
    )?;
    let username = match &args.username {
        Some(user) => user.clone(),
        None => ask("Gatehouse username: ")?,
    };
    if username.trim().is_empty() {
        return Err("a Gatehouse username is required".to_string());
    }
    let password = gatehouse_auth::prompt_password("Password: ").map_err(|e| e.to_string())?;
    let host = std::env::var("HOSTNAME")
        .ok()
        .filter(|h| !h.is_empty())
        .unwrap_or_else(|| "this machine".to_string());
    let credentials =
        gatehouse_auth::login(&url, username.trim(), &password, &new_token(args, &host))
            .map_err(|e| e.to_string())?;
    std::fs::create_dir_all(config_dir)
        .map_err(|e| format!("cannot create {}: {e}", config_dir.display()))?;
    gatehouse_auth::save(TOOL, config_dir, &credentials).map_err(|e| e.to_string())
}
