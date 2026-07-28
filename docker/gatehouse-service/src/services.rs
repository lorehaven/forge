//! The estate gatehouse knows how to send you to.
//!
//! A service appears on the home page when its URL is configured and its
//! feature flag is not turned off. That way a deployment lists exactly what it
//! actually runs, and gatehouse needs no code change to gain or lose one.

pub struct ServiceLink {
    pub url: String,
    pub title_key: &'static str,
    pub desc_key: &'static str,
    pub card_class: &'static str,
}

struct ServiceDefinition {
    /// Environment prefix, e.g. `SAGE` for `SAGE_UI_URL` / `FEATURE_SAGE_ENABLED`.
    env_prefix: &'static str,
    title_key: &'static str,
    desc_key: &'static str,
    card_class: &'static str,
}

const SERVICES: [ServiceDefinition; 4] = [
    ServiceDefinition {
        env_prefix: "CONVEYOR",
        title_key: "ui_service_conveyor_title",
        desc_key: "ui_service_conveyor_desc",
        card_class: "home-card-conveyor",
    },
    ServiceDefinition {
        env_prefix: "SAGE",
        title_key: "ui_service_sage_title",
        desc_key: "ui_service_sage_desc",
        card_class: "home-card-sage",
    },
    ServiceDefinition {
        env_prefix: "SWITCHBOARD",
        title_key: "ui_service_switchboard_title",
        desc_key: "ui_service_switchboard_desc",
        card_class: "home-card-switchboard",
    },
    ServiceDefinition {
        env_prefix: "WAREHOUSE",
        title_key: "ui_service_warehouse_title",
        desc_key: "ui_service_warehouse_desc",
        card_class: "home-card-warehouse",
    },
];

/// Services to offer, in declaration order.
pub fn enabled_services() -> Vec<ServiceLink> {
    SERVICES
        .iter()
        .filter_map(|service| {
            let url = service_url(service.env_prefix)?;
            feature_enabled(&format!("FEATURE_{}_ENABLED", service.env_prefix), true).then_some(
                ServiceLink {
                    url,
                    title_key: service.title_key,
                    desc_key: service.desc_key,
                    card_class: service.card_class,
                },
            )
        })
        .collect()
}

/// `<PREFIX>_UI_URL`, falling back to `<PREFIX>_URL` so a deployment that
/// already points gatehouse at a service does not have to repeat itself.
fn service_url(prefix: &str) -> Option<String> {
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
fn feature_enabled(name: &str, default: bool) -> bool {
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
