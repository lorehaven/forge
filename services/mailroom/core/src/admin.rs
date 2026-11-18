//! What the administrator's panel reads and writes.

use quench_serde::{Deserialize, Serialize};

/// The administrator's settings.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Settings {
    /// The part of every address after the `@` (set in the deployment, shown here).
    pub domain: String,
    /// Mail the services may deliver per day, all together.
    pub daily_budget: u32,
    /// Mail one service may deliver per hour.
    pub hourly_per_source: u32,
    /// Messages one person may send per hour.
    pub hourly_per_person: u32,
    /// What one mailbox may hold, in megabytes (mail and attachments).
    pub quota_mb: u32,
}

/// A change to the settings. `None` leaves a part alone.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SettingsInput {
    #[serde(default)]
    pub daily_budget: Option<u32>,
    #[serde(default)]
    pub hourly_per_source: Option<u32>,
    #[serde(default)]
    pub hourly_per_person: Option<u32>,
    #[serde(default)]
    pub quota_mb: Option<u32>,
}

/// A mailbox as the administrator sees it: whose it is and how full, never what is in it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminMailbox {
    pub owner: String,
    pub address: String,
    pub messages: u32,
    pub bytes: u64,
    pub created: i64,
    #[serde(default)]
    pub last_seen: Option<i64>,
    /// A disabled mailbox still receives mail but its owner cannot open it or send from it.
    pub disabled: bool,
}

/// One delivery a service made: who asked, to whom, what for. Mail between people is not listed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Delivery {
    pub id: String,
    /// The service.
    pub source: String,
    /// What sort of message (`verification`, ...), when the service said.
    pub kind: String,
    pub recipients: Vec<String>,
    pub subject: String,
    /// `delivered`, `partial` (some recipients had no mailbox and were refused) or `failed`.
    pub status: String,
    pub delivered: u32,
    #[serde(default)]
    pub error: Option<String>,
    pub created: i64,
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
    pub mailboxes: u32,
    pub messages: u32,
    pub bytes: u64,
    pub delivered_24h: u32,
    pub failed_24h: u32,
    pub budget_left: u32,
}

/// A page of anything with a total.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub total: u32,
}
