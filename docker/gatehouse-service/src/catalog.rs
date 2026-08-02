//! The realm's permission catalog.
//!
//! Which services exist, what actions each supports, and the named grant
//! bundles ("templates") an admin can assign - all read from
//! `config/permissions.toml` (see that file for the schema and the reasoning).
//! This is what keeps a new service, or a new action on an existing one, out
//! of gatehouse's Rust code: both used to be either an env var
//! (`SERVICE_AUDIENCES`, fine on its own) or a hardcoded two-value enum (not
//! fine - the same two options for every service, changeable only by a
//! gatehouse release). Now both live in one file an operator edits and
//! restarts gatehouse to pick up.

use quench_auth::prelude::{Actions, Permissions};
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Deserialize)]
struct ServiceEntry {
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    actions: Vec<String>,
    /// Kinds of resource this service accepts a scoped grant on - e.g.
    /// conveyor's `project`. Declaring one here is what makes
    /// `is_known_action` accept `<resource_type>:<resource_id>:<action>` as
    /// well as a plain action name; the resource id itself is never
    /// validated against anything, the same way `repos.id` is an opaque
    /// string to everything that isn't conveyor.
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
    /// Reads `PERMISSIONS_CONFIG` (default `config/permissions.toml`, relative
    /// to gatehouse's working directory - the same convention `cert.pem` and
    /// `i18n/` already use).
    ///
    /// Fails loudly rather than falling back to an empty catalog: a realm with
    /// no grantable services is not a smaller version of the estate, it is a
    /// broken one, and that should stop gatehouse from starting rather than
    /// come up quietly unable to grant anything.
    pub fn load() -> anyhow::Result<Self> {
        Self::load_from(&envmnt::get_or(
            "PERMISSIONS_CONFIG",
            "config/permissions.toml",
        ))
    }

    /// The path-explicit half of `load`, split out so tests do not have to
    /// race each other over a process-global environment variable to point at
    /// their own fixture file.
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

    /// Every service in the catalog, in file order. This is also the realm's
    /// audience list - see `main.rs`, which sets `JwtConfig::audiences` from
    /// it rather than from `SERVICE_AUDIENCES`.
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

    /// Whether `action` is grantable on `service` - either a plain action
    /// this service's catalog entry enumerates, or a resource-scoped grant
    /// shaped `<resource_type>:<resource_id>:<base_action>` where
    /// `resource_type` is one this service declares and `base_action` is
    /// itself one of the enumerated actions. The resource id in the middle is
    /// deliberately unchecked - gatehouse has no way to know whether a given
    /// conveyor project id exists, and does not need to: an admin naming one
    /// that does not exist just grants access to nothing, the same safe
    /// direction an unparseable permissions row already fails in.
    pub fn is_known_action(&self, service: &str, action: &str) -> bool {
        if self.actions_for(service).iter().any(|known| known == action) {
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

    /// `service` and `service:action` entries a grant map holds that this
    /// catalog does not recognise. Empty when everything checks out.
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

    /// What a self-registered account starts with. Empty if no default
    /// template is configured - checked at load time, so that can only happen
    /// deliberately, not because of a typo.
    pub fn default_registration_grants(&self) -> Permissions {
        self.default_registration_template
            .as_deref()
            .and_then(|name| self.template(name))
            .cloned()
            .unwrap_or_default()
    }

    /// Every template must only grant known services/actions, and the default
    /// registration template (if any) must exist - checked once here so a
    /// typo in the catalog fails startup instead of silently granting nothing,
    /// or nothing useful, to whoever hits it first.
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A fixture file per call, under a name unique enough that concurrent
    /// tests never collide - `load_from` takes an explicit path precisely so
    /// this does not have to go through a process-global environment variable.
    fn load(toml: &str) -> anyhow::Result<PermissionCatalog> {
        let dir = std::env::temp_dir().join(format!("permcat-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("permissions.toml");
        std::fs::write(&path, toml).unwrap();
        let result = PermissionCatalog::load_from(&path.to_string_lossy());
        let _ = std::fs::remove_dir_all(&dir);
        result
    }

    #[test]
    fn a_service_lists_its_declared_actions() {
        let catalog = load(
            r#"
            [services.sage]
            actions = ["read", "write"]
            "#,
        )
        .unwrap();

        assert_eq!(catalog.actions_for("sage"), ["read", "write"]);
        assert!(catalog.is_known_action("sage", "write"));
        assert!(!catalog.is_known_action("sage", "delete-everything"));
        assert!(!catalog.is_known_service("warehouse"));
    }

    #[test]
    fn a_resource_scoped_action_is_known_when_its_type_and_base_action_are() {
        let catalog = load(
            r#"
            [services.conveyor]
            actions = ["read", "write"]
            resource_types = ["project"]
            "#,
        )
        .unwrap();

        assert!(catalog.is_known_action("conveyor", "project:abc-123:write"));
        assert!(catalog.is_known_action("conveyor", "project:abc-123:read"));
        // The resource id itself is never validated - any string in the
        // middle segment is accepted.
        assert!(catalog.is_known_action("conveyor", "project:does-not-exist:read"));
        // An undeclared resource type, or a base action the service does not
        // grant, is still rejected.
        assert!(!catalog.is_known_action("conveyor", "repo:abc-123:read"));
        assert!(!catalog.is_known_action("conveyor", "project:abc-123:launch"));
    }

    #[test]
    fn a_template_expands_to_a_permissions_map() {
        let catalog = load(
            r#"
            [services.sage]
            actions = ["read", "write"]
            [services.warehouse]
            actions = ["read", "write"]
            [templates.viewer]
            sage = ["read"]
            warehouse = ["read"]
            "#,
        )
        .unwrap();

        let viewer = catalog.template("viewer").unwrap();
        assert_eq!(
            viewer.get("sage").cloned().unwrap_or_default(),
            ["read"].map(str::to_string).into()
        );
        assert!(catalog.template("nonexistent").is_none());
    }

    #[test]
    fn a_template_naming_an_unknown_action_fails_to_load() {
        let err = load(
            r#"
            [services.sage]
            actions = ["read"]
            [templates.editor]
            sage = ["write"]
            "#,
        )
        .unwrap_err();

        assert!(err.to_string().contains("sage:write"));
    }

    #[test]
    fn a_dangling_default_template_fails_to_load() {
        let err = load(
            r#"
            [services.sage]
            actions = ["read"]
            [registration]
            default_template = "nonexistent"
            "#,
        )
        .unwrap_err();

        assert!(err.to_string().contains("nonexistent"));
    }

    #[test]
    fn no_default_template_means_registration_grants_nothing() {
        let catalog = load(
            r#"
            [services.sage]
            actions = ["read"]
            "#,
        )
        .unwrap();

        assert!(catalog.default_registration_grants().is_empty());
    }

    #[test]
    fn an_empty_catalog_fails_to_load() {
        let err = load("").unwrap_err();
        assert!(err.to_string().contains("no [services"));
    }
}
