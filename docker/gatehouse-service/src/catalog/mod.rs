//! The realm's permission catalog - services, actions, and grant templates,
//! read from `config/permissions.toml` rather than hardcoded in Rust.

use quench_auth::prelude::{Actions, Permissions};
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Deserialize)]
struct ServiceEntry {
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    actions: Vec<String>,
    /// Resource kinds this service accepts a scoped grant on (e.g. conveyor's
    /// `project`) - enables `<resource_type>:<resource_id>:<action>` grants.
    #[serde(default)]
    resource_types: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct RegistrationEntry {
    #[serde(default)]
    default_template: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct PermissionsFile {
    #[serde(default)]
    services: BTreeMap<String, ServiceEntry>,
    #[serde(default)]
    templates: BTreeMap<String, BTreeMap<String, Vec<String>>>,
    #[serde(default)]
    registration: RegistrationEntry,
}

#[derive(Debug, Clone)]
pub struct PermissionCatalog {
    services: BTreeMap<String, ServiceEntry>,
    templates: BTreeMap<String, Permissions>,
    default_registration_template: Option<String>,
}

impl PermissionCatalog {
    /// Reads `PERMISSIONS_CONFIG` (default `config/permissions.toml`). Fails
    /// loudly rather than starting with an empty, broken catalog.
    pub fn load() -> anyhow::Result<Self> {
        Self::load_from(&envmnt::get_or(
            "PERMISSIONS_CONFIG",
            "config/permissions.toml",
        ))
    }

    /// Path-explicit half of `load`, so tests don't race over the env var.
    pub fn load_from(path: &str) -> anyhow::Result<Self> {
        let file: PermissionsFile = quench_config::ConfigLoader::from_toml_file(path)
            .map_err(|err| anyhow::anyhow!("failed to load permission catalog {path}: {err}"))?;

        let templates = file
            .templates
            .into_iter()
            .map(|(name, grants)| {
                let grants: Permissions = grants
                    .into_iter()
                    .map(|(service, actions)| {
                        let actions: Actions = actions.into_iter().collect();
                        (service, actions)
                    })
                    .collect();
                (name, grants)
            })
            .collect();

        let catalog = Self {
            services: file.services,
            templates,
            default_registration_template: file.registration.default_template,
        };
        catalog.validate()?;
        Ok(catalog)
    }

    /// Every service, in file order - also the realm's audience list (see `main.rs`).
    pub fn service_names(&self) -> impl Iterator<Item = &str> {
        self.services.keys().map(String::as_str)
    }

    pub fn label<'a>(&'a self, service: &'a str) -> &'a str {
        self.services
            .get(service)
            .and_then(|entry| entry.label.as_deref())
            .unwrap_or(service)
    }

    pub fn actions_for(&self, service: &str) -> &[String] {
        self.services
            .get(service)
            .map(|entry| entry.actions.as_slice())
            .unwrap_or(&[])
    }

    pub fn resource_types_for(&self, service: &str) -> &[String] {
        self.services
            .get(service)
            .map(|entry| entry.resource_types.as_slice())
            .unwrap_or(&[])
    }

    pub fn is_known_service(&self, service: &str) -> bool {
        self.services.contains_key(service)
    }

    /// A plain enumerated action, or `<resource_type>:<resource_id>:<base_action>`
    /// - the resource id itself is deliberately unchecked.
    pub fn is_known_action(&self, service: &str, action: &str) -> bool {
        if self
            .actions_for(service)
            .iter()
            .any(|known| known == action)
        {
            return true;
        }

        let Some((resource_type, rest)) = action.split_once(':') else {
            return false;
        };
        let Some((_resource_id, base_action)) = rest.split_once(':') else {
            return false;
        };

        self.resource_types_for(service)
            .iter()
            .any(|known| known == resource_type)
            && self
                .actions_for(service)
                .iter()
                .any(|known| known == base_action)
    }

    /// Unrecognised `service`/`service:action` entries; empty if all check out.
    pub fn unknown_grants(&self, permissions: &Permissions) -> Vec<String> {
        let mut unknown = Vec::new();
        for (service, actions) in permissions {
            if !self.is_known_service(service) {
                unknown.push(service.clone());
                continue;
            }
            for action in actions {
                if !self.is_known_action(service, action) {
                    unknown.push(format!("{service}:{action}"));
                }
            }
        }
        unknown
    }

    pub fn template(&self, name: &str) -> Option<&Permissions> {
        self.templates.get(name)
    }

    pub fn template_names(&self) -> impl Iterator<Item = &str> {
        self.templates.keys().map(String::as_str)
    }

    /// What a self-registered account starts with; empty if no default template.
    pub fn default_registration_grants(&self) -> Permissions {
        self.default_registration_template
            .as_deref()
            .and_then(|name| self.template(name))
            .cloned()
            .unwrap_or_default()
    }

    /// Catalog-wide checks so a typo fails startup, not a request.
    fn validate(&self) -> anyhow::Result<()> {
        if self.services.is_empty() {
            anyhow::bail!("permission catalog has no [services.*] entries");
        }

        for (name, grants) in &self.templates {
            let unknown = self.unknown_grants(grants);
            if !unknown.is_empty() {
                anyhow::bail!(
                    "template '{name}' grants unknown service/action(s): {}",
                    unknown.join(", ")
                );
            }
        }

        if let Some(default) = &self.default_registration_template
            && !self.templates.contains_key(default)
        {
            anyhow::bail!("registration.default_template '{default}' is not a known template");
        }

        Ok(())
    }
}
