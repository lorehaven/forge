//! `riveter login` - mint an application token and keep it in `.riveter.secret.toml`.

use anyhow::{Context, Result, bail};
use gatehouse_auth::NewToken;
use std::io::{self, Write as _};

const TOOL: &str = "riveter";

/// What `riveter login` was asked for.
#[derive(Debug, Default, Clone)]
pub struct LoginArgs {
    pub gatehouse_url: Option<String>,
    pub username: Option<String>,
    pub name: Option<String>,
    pub scope: Vec<String>,
    pub days: Option<i64>,
}

#[must_use]
pub fn default_token_name(host: &str) -> String {
    format!("riveter on {host}")
}

pub fn login_command(args: &LoginArgs) -> Result<()> {
    let dir = std::env::current_dir().context("finding the working directory")?;
    let url = match &args.gatehouse_url {
        Some(url) => url.clone(),
        None => gatehouse_auth::load(TOOL, &dir)?
            .map(|stored| stored.gatehouse_url)
            .context("pass --gatehouse-url (e.g. https://host/gatehouse)")?,
    };
    let username = match &args.username {
        Some(username) => username.clone(),
        None => prompt("Gatehouse username: ")?,
    };
    let password = gatehouse_auth::prompt_password("Password: ")?;
    let host = std::env::var("HOSTNAME")
        .ok()
        .filter(|host| !host.is_empty())
        .unwrap_or_else(|| "this machine".to_string());
    let new = NewToken {
        name: args
            .name
            .clone()
            .unwrap_or_else(|| default_token_name(&host)),
        scope: args.scope.clone(),
        expires_in_days: args.days,
    };

    let credentials = gatehouse_auth::login(&url, &username, &password, &new)?;
    let path = gatehouse_auth::save(TOOL, &dir, &credentials)?;
    println!("Stored an application token in {}", path.display());
    println!("It is git-ignored; revoke it from Gatehouse if the machine is lost.");
    Ok(())
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
