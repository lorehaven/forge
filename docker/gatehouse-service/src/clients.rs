//! OAuth clients: the relying parties (browser authorization-code + PKCE
//! flow) and machine identities (`client_credentials`) gatehouse issues
//! tokens to. Seeded from `config/clients.toml` at every boot - a fixed,
//! small set of clients the estate already knows about, not something worth
//! a management UI for. Unlike a user's password, the config file stays the
//! source of truth, so a changed secret or redirect URI takes effect on the
//! next restart rather than being ignored the way `bootstrap::seed_users`
//! ignores a changed `SERVICE_PASSWORD`.

use chrono::{DateTime, Utc};
use quench_db::prelude::{Crud, Db, Model, Repository};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ClientRow {
    pub client_id: String,
    pub secret_hash: String,
    pub redirect_uris: Vec<String>,
    pub allowed_scopes: Vec<String>,
    pub created_at: DateTime<Utc>,
}

impl Model for ClientRow {
    fn table_name() -> String {
        format!("{}.clients", quench_auth::prelude::realm::auth_schema())
    }

    fn columns() -> Vec<&'static str> {
        vec![
            "client_id",
            "secret_hash",
            "redirect_uris",
            "allowed_scopes",
            "created_at",
        ]
    }

    fn primary_key_name() -> String {
        "client_id".to_string()
    }
}

impl ClientRow {
    pub fn secret_matches(&self, candidate: &str) -> bool {
        self.secret_hash == hash_secret(candidate)
    }

    pub fn redirect_uri_matches(&self, candidate: &str) -> bool {
        self.redirect_uris.iter().any(|uri| uri == candidate)
    }
}

pub fn hash_secret(secret: &str) -> String {
    hex::encode(Sha256::digest(secret.as_bytes()))
}

#[derive(Debug, Clone, Deserialize)]
struct ClientEntry {
    client_id: String,
    /// Appended to the client's own `<PREFIX>_UI_URL`/`<PREFIX>_URL` (the same
    /// env vars `services.rs` reads for the home page). Absent for a
    /// `client_credentials`-only client, which never redirects a browser.
    #[serde(default)]
    redirect_path: Option<String>,
    #[serde(default)]
    allowed_scopes: Vec<String>,
    /// The env var gatehouse itself reads to get this client's secret - the
    /// value lives once in gatehouse's own env and is mirrored into the
    /// owning service's, the same "one value, two places" pattern
    /// `JWT_SECRET` used before, now scoped per client.
    secret_env: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct ClientsFile {
    #[serde(default)]
    client: Vec<ClientEntry>,
}

/// Reads `CLIENTS_CONFIG` (default `config/clients.toml`) and upserts every
/// entry whose secret is actually configured. A client with no secret set
/// (e.g. a deployment that never enabled conveyor) is skipped with a warning
/// rather than seeded with an empty, guessable one.
pub async fn seed_clients(db: &Db) -> anyhow::Result<()> {
    let path = envmnt::get_or("CLIENTS_CONFIG", "config/clients.toml");
    let file: ClientsFile = quench_config::ConfigLoader::from_toml_file(&path)
        .map_err(|err| anyhow::anyhow!("failed to load client catalog {path}: {err}"))?;

    let repo = db.repository::<ClientRow>();
    for entry in file.client {
        let secret = envmnt::get_or(&entry.secret_env, "");
        if secret.trim().is_empty() {
            tracing::warn!(
                "{} not set: skipping client '{}' (nothing can complete its token exchange)",
                entry.secret_env,
                entry.client_id
            );
            continue;
        }

        let redirect_uris = match &entry.redirect_path {
            Some(path) => match redirect_base_url(&entry.client_id) {
                Some(base) => vec![format!("{base}{path}")],
                None => {
                    tracing::warn!(
                        "no <PREFIX>_UI_URL/<PREFIX>_URL configured for client '{}': it has no \
                         redirect_uri, so the authorization-code flow will reject it",
                        entry.client_id
                    );
                    vec![]
                }
            },
            None => vec![],
        };

        let row = ClientRow {
            client_id: entry.client_id.clone(),
            secret_hash: hash_secret(&secret),
            redirect_uris,
            allowed_scopes: entry.allowed_scopes,
            created_at: Utc::now(),
        };

        upsert(&repo, row).await?;
    }

    Ok(())
}

async fn upsert(repo: &Repository<ClientRow>, row: ClientRow) -> anyhow::Result<()> {
    let existing = repo.read(&row.client_id).await?;
    if existing.is_some() {
        repo.update(&row).await?;
    } else {
        repo.create(&row).await?;
    }
    Ok(())
}

/// `<PREFIX>_UI_URL`/`<PREFIX>_URL`, uppercased from the client id, with any
/// trailing `/ui/home` (what those vars point at today, for the home-page
/// cards) trimmed back to the service's own base.
fn redirect_base_url(client_id: &str) -> Option<String> {
    let prefix = client_id.to_uppercase().replace('-', "_");
    for key in [format!("{prefix}_UI_URL"), format!("{prefix}_URL")] {
        let value = envmnt::get_or(&key, "");
        let trimmed = value.trim().trim_end_matches('/');
        if !trimmed.is_empty() {
            return Some(
                trimmed
                    .strip_suffix("/ui/home")
                    .unwrap_or(trimmed)
                    .to_string(),
            );
        }
    }
    None
}
