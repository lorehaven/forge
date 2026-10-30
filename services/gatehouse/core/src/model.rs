//! What the page API says. The service builds these from the realm; the client reads them.

use quench_serde::{Deserialize, Serialize};

/// A card on the services dashboard: one service this deployment offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceCard {
    pub url: String,
    /// `ui_service_<name>_title` / `_desc`: translations, when a locale has them.
    pub title_key: String,
    pub desc_key: String,
    /// Shown when no translation applies.
    pub label: String,
    #[serde(default)]
    pub description: Option<String>,
    /// `home-card-<name>`.
    pub card_class: String,
}

impl ServiceCard {
    /// The service's short name (`conveyor`), from its card class (`home-card-conveyor`).
    #[must_use]
    pub fn slug(&self) -> &str {
        self.card_class
            .strip_prefix("home-card-")
            .unwrap_or(&self.card_class)
    }
}

/// The services whose icons the page serves (`/ui/assets/icons/<name>.png`).
pub const ICONS: [&str; 9] = [
    "conveyor",
    "daybook",
    "gantry",
    "gauge",
    "hoist",
    "sage",
    "switchboard",
    "warehouse",
    "workbench",
];

/// The dashboard: the services the person's deployment offers, and whether the realm's own section is there.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dashboard {
    /// Whether the person is an administrator (the user list is offered).
    pub admin: bool,
    pub services: Vec<ServiceCard>,
}

/// One kind of service notification and whether the person wants it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Subscription {
    pub id: String,
    pub service: String,
    /// Translation key of the kind's name.
    pub label_key: String,
    pub wanted: bool,
}

/// The person's own account page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub username: String,
    #[serde(default)]
    pub display_name: Option<String>,
    /// A pasted picture address; never the bytes of an uploaded one.
    #[serde(default)]
    pub avatar_url: Option<String>,
    /// Whether there is a picture at all (pasted or uploaded).
    pub has_avatar: bool,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub timezone: Option<String>,
    #[serde(default)]
    pub preferred_locale: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    pub email_verified: bool,
    pub mfa_enabled: bool,
    /// Whether a confirmed address exists for notices to go to.
    pub has_notice_address: bool,
    pub timezones: Vec<String>,
    pub locales: Vec<String>,
    pub subscriptions: Vec<Subscription>,
}

/// The secret and code for enrolling an authenticator.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MfaEnrollment {
    pub secret: String,
    pub uri: String,
    /// The provisioning URI as a scannable image (`data:` URI), when one could be drawn.
    #[serde(default)]
    pub qr: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MfaEnrollRequest {
    pub secret: String,
    pub code: String,
}

/// A service and the actions held on it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grant {
    pub service: String,
    pub actions: Vec<String>,
}

/// One line of the user list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminUserRow {
    pub username: String,
    pub roles: Vec<String>,
    /// A role grants everything.
    pub wildcard: bool,
    pub grants: Vec<Grant>,
}

/// The user list, and what the person may do with it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminUsers {
    pub actor: String,
    /// Whether the person is a literal administrator (may pick roles).
    pub admin: bool,
    pub can_create: bool,
    pub users: Vec<AdminUserRow>,
}

/// A service of the permission catalog and the actions that can be granted on it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogService {
    pub name: String,
    pub label: String,
    pub actions: Vec<String>,
}

/// One user as the administrator sees them, with what the viewer may do about it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminUser {
    pub username: String,
    /// `user`, `admin` or `service`: the most privileged role held.
    pub role: String,
    pub wildcard: bool,
    pub held: Vec<Grant>,
    #[serde(default)]
    pub email: Option<String>,
    pub email_verified: bool,
    pub disabled: bool,
    pub locked: bool,
    pub mfa_enabled: bool,
    /// `2026-10-29 10:00 UTC`.
    pub created_at: String,
    #[serde(default)]
    pub last_login_at: Option<String>,
    /// Whether this is the viewer's own account (no self-disable, no self-delete).
    pub is_self: bool,
    pub viewer_admin: bool,
    pub can_edit: bool,
    pub can_delete: bool,
    pub can_manage_permissions: bool,
    pub services: Vec<CatalogService>,
    pub templates: Vec<String>,
}

/// The new address a confirmation link would set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfirmEmail {
    pub new_email: String,
}

/// The kind of notification an unsubscribe link ends.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnsubscribeInfo {
    pub label_key: String,
}
