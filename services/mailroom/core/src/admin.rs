//! What the administrator's panel reads and writes.

use quench_serde::{Deserialize, Serialize};

/// The estate's outgoing mail server (what services send through).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transport {
    pub host: String,
    pub port: u16,
    pub security: String,
    pub username: String,
    /// Whether a password is stored (it is never sent back).
    pub has_password: bool,
    #[serde(default)]
    pub custom_certificate: bool,
}

/// An address services may send as.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Identity {
    pub address: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub default: bool,
}

/// The administrator's settings.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub transport: Option<Transport>,
    pub identities: Vec<Identity>,
    /// Hosts that may be private addresses (an in-cluster Proton Bridge), exact or `*.suffix`.
    pub private_hosts: Vec<String>,
    /// Ports allowed besides 143, 993, 465, 587 and 2525.
    pub extra_ports: Vec<u16>,
    /// Mail the estate may send per day.
    pub daily_budget: u32,
    /// Mail one service may send per hour.
    pub hourly_per_source: u32,
    pub max_accounts_per_user: u32,
    pub sync_interval_secs: u32,
}

/// A change to the settings. `None` leaves a part alone.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SettingsInput {
    #[serde(default)]
    pub transport: Option<TransportInput>,
    #[serde(default)]
    pub identities: Option<Vec<Identity>>,
    #[serde(default)]
    pub private_hosts: Option<Vec<String>>,
    #[serde(default)]
    pub extra_ports: Option<Vec<u16>>,
    #[serde(default)]
    pub daily_budget: Option<u32>,
    #[serde(default)]
    pub hourly_per_source: Option<u32>,
    #[serde(default)]
    pub max_accounts_per_user: Option<u32>,
    #[serde(default)]
    pub sync_interval_secs: Option<u32>,
}

/// The transport as typed; an empty password keeps the stored one.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TransportInput {
    pub host: String,
    pub port: u16,
    pub security: String,
    pub username: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub tls_roots_pem: String,
}

/// A mailbox as the administrator sees it: who owns it and how it is doing, never its mail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminAccount {
    pub id: String,
    pub owner: String,
    pub label: String,
    pub address: String,
    pub imap_host: String,
    pub state: String,
    #[serde(default)]
    pub last_error: Option<String>,
    #[serde(default)]
    pub last_sync: Option<i64>,
    pub messages: u32,
    pub disabled: bool,
}

/// A message waiting to go out, or one that went (or failed). No body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutboxItem {
    pub id: String,
    /// The service or person it came from.
    pub source: String,
    /// `service` or `user`.
    pub kind: String,
    pub recipients: Vec<String>,
    pub subject: String,
    /// `queued`, `sending`, `sent` or `failed`.
    pub status: String,
    pub attempts: u32,
    #[serde(default)]
    pub last_error: Option<String>,
    pub created: i64,
    #[serde(default)]
    pub next_attempt: Option<i64>,
    #[serde(default)]
    pub sent: Option<i64>,
}

/// One line of what administrators did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: String,
    pub at: i64,
    pub actor: String,
    pub action: String,
    pub target: String,
    #[serde(default)]
    pub detail: String,
}

/// Counters for the panel's front page.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Stats {
    pub accounts: u32,
    pub accounts_failing: u32,
    pub messages: u32,
    pub queued: u32,
    pub sent_24h: u32,
    pub failed_24h: u32,
    pub budget_left: u32,
}

/// A page of anything with a total.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub total: u32,
}
