//! OAuth clients (relying parties + machine identities) gatehouse issues
//! tokens to, seeded from `config/clients.toml` on every boot.

use quench_core::time::Time;
use quench_db::prelude::{Crud, Db, Model, Repository};
use quench_serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientRow {
    pub client_id: String,
    pub secret_hash: String,
    pub redirect_uris: Vec<String>,
    pub allowed_scopes: Vec<String>,
    pub created_at: Time,
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
    #[must_use]
    pub fn secret_matches(&self, candidate: &str) -> bool {
        self.secret_hash == hash_secret(candidate)
    }

    #[must_use]
    pub fn redirect_uri_matches(&self, candidate: &str) -> bool {
        self.redirect_uris.iter().any(|uri| uri == candidate)
    }
}

#[must_use]
pub fn hash_secret(secret: &str) -> String {
    quench_core::hex::encode(quench_crypto::hash::sha256(secret.as_bytes()))
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
pub async fn seed_clients(db: &Db) -> quench_core::Result<()> {
    let path = quench_core::env::get_or("CLIENTS_CONFIG", "config/clients.toml");
    let file: ClientsFile = quench_config::ConfigLoader::from_toml_file(&path)
        .map_err(|err| quench_core::err!("failed to load client catalog {path}: {err}"))?;

    let repo = db.repository::<ClientRow>();
    for entry in file.client {
        let secret = quench_core::env::get_or(&entry.secret_env, "");
        if secret.trim().is_empty() {
            quench_log::warn!(
                "{} not set: skipping client '{}' (nothing can complete its token exchange)",
                entry.secret_env,
                entry.client_id
            );
            continue;
        }

        let redirect_uris = match &entry.redirect_path {
            Some(path) => {
                if let Some(base) = redirect_base_url(&entry.client_id) {
                    vec![format!("{base}{path}")]
                } else {
                    quench_log::warn!(
                        "no <PREFIX>_UI_URL/<PREFIX>_URL configured for client '{}': it has no \
                     redirect_uri, so the authorization-code flow will reject it",
                        entry.client_id
                    );
                    vec![]
                }
            }
            None => vec![],
        };

        let row = ClientRow {
            client_id: entry.client_id.clone(),
            secret_hash: hash_secret(&secret),
            redirect_uris,
            allowed_scopes: entry.allowed_scopes,
            created_at: Time::now(),
        };

        upsert(&repo, row).await?;
    }

    Ok(())
}

pub async fn upsert(repo: &Repository<ClientRow>, row: ClientRow) -> quench_core::Result<()> {
    let existing = repo.read(&row.client_id).await?;
    if existing.is_some() {
        repo.update(&row).await?;
    } else {
        repo.create(&row).await?;
    }
    Ok(())
}

/// `<PREFIX>_UI_URL`/`<PREFIX>_URL`, with any trailing `/ui/home` trimmed.
#[must_use]
pub fn redirect_base_url(client_id: &str) -> Option<String> {
    let prefix = client_id.to_uppercase().replace('-', "_");
    for key in [format!("{prefix}_UI_URL"), format!("{prefix}_URL")] {
        let value = quench_core::env::get_or(&key, "");
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
