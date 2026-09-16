//! OAuth clients (relying parties + machine identities) gatehouse issues
//! tokens to, seeded from `config/clients.toml` on every boot.

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
    /// Appended to `<PREFIX>_UI_URL`/`<PREFIX>_URL`. Absent for `client_credentials`-only.
    #[serde(default)]
    redirect_path: Option<String>,
    #[serde(default)]
    allowed_scopes: Vec<String>,
    /// Env var gatehouse reads for this client's secret.
    secret_env: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct ClientsFile {
    #[serde(default)]
    client: Vec<ClientEntry>,
}

/// Reads `CLIENTS_CONFIG` and upserts every entry with a configured secret;
/// an unconfigured one is skipped with a warning, not seeded with a guessable one.
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

pub async fn upsert(repo: &Repository<ClientRow>, row: ClientRow) -> anyhow::Result<()> {
    let existing = repo.read(&row.client_id).await?;
    if existing.is_some() {
        repo.update(&row).await?;
    } else {
        repo.create(&row).await?;
    }
    Ok(())
}

/// `<PREFIX>_UI_URL`/`<PREFIX>_URL`, with any trailing `/ui/home` trimmed.
pub fn redirect_base_url(client_id: &str) -> Option<String> {
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
