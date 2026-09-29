//! Services gatehouse can send you to - the catalog says which exist and want a
//! card (`home = true`); the environment says where they are and whether they
//! are switched on. A card shows when its URL is configured and its feature
//! flag is not off.

use crate::catalog::PermissionCatalog;

pub struct ServiceLink {
    pub url: String,
    /// `ui_service_<name>_title` / `_desc`: translations, when a locale has them.
    pub title_key: String,
    pub desc_key: String,
    /// Shown until a translation applies, and for a service with none.
    pub label: String,
    pub description: Option<String>,
    pub card_class: String,
}

/// `workbench` -> `WORKBENCH`, for `WORKBENCH_UI_URL` / `FEATURE_WORKBENCH_ENABLED`.
fn env_prefix(name: &str) -> String {
    name.to_ascii_uppercase().replace('-', "_")
}

/// Services to offer, in the catalog's order.
pub fn enabled_services(catalog: &PermissionCatalog) -> Vec<ServiceLink> {
    catalog
        .home_services()
        .into_iter()
        .filter_map(|service| {
            let prefix = env_prefix(service.name);
            let url = service_url(&prefix)?;
            feature_enabled(&format!("FEATURE_{prefix}_ENABLED"), true).then(|| ServiceLink {
                url,
                title_key: format!("ui_service_{}_title", service.name),
                desc_key: format!("ui_service_{}_desc", service.name),
                label: service.label.to_string(),
                description: service.description.map(str::to_string),
                card_class: format!("home-card-{}", service.name),
            })
        })
        .collect()
}

/// `<PREFIX>_UI_URL`, falling back to `<PREFIX>_URL` so a deployment that
/// already points gatehouse at a service does not have to repeat itself.
pub fn service_url(prefix: &str) -> Option<String> {
    for key in [format!("{prefix}_UI_URL"), format!("{prefix}_URL")] {
        let value = envmnt::get_or(&key, "");
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.trim_end_matches('/').to_string());
        }
    }
    None
}

/// Matches how the other services read their feature flags.
pub fn feature_enabled(name: &str, default: bool) -> bool {
    match envmnt::get_or(name, if default { "true" } else { "false" })
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "1" | "true" | "yes" | "on" => true,
        "0" | "false" | "no" | "off" => false,
        _ => default,
    }
}
