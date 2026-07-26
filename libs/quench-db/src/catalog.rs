//! The migration catalog: a directory of independently versioned modules.
//!
//! ```text
//! migrations/
//!   quench-auth/
//!     module.toml            # name, version, requires, defaults
//!     0001-users.toml        # migrations, each tagged with `since`
//!     0002-sessions.toml
//!   sage/
//!     module.toml
//!     ...
//! ```
//!
//! A module is installed into a target schema; the same module can be installed
//! into several schemas of the same database (that is how `quench-auth` serves
//! `sage`, `switchboard` and `warehouse` at once).

use crate::migrations::{Migration, MigrationLoader};
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const MODULE_MANIFEST: &str = "module.toml";

/// Whether a module's state is tracked per schema or once per database.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModuleScope {
    /// Installed once per target schema (the default).
    #[default]
    Schema,
    /// Installed once per database - extensions, roles, and other cluster-wide
    /// objects that do not live in a schema.
    Database,
}

/// A dependency on another module.
///
/// TOML accepts either the short form or the table form:
///
/// ```toml
/// requires = [
///   "auth@^0.1",
///   { module = "pgvector", version = "^0.1", schema = "public" },
/// ]
/// ```
///
/// Without an explicit `schema` the dependency is installed into the same
/// schema as the module that requires it.
#[derive(Debug, Clone)]
pub struct Requirement {
    pub module: String,
    pub version: VersionReq,
    pub schema: Option<String>,
}

impl Requirement {
    fn parse_short(spec: &str) -> Result<Self, CatalogError> {
        let (module, version) = match spec.split_once('@') {
            Some((m, v)) => (m.trim(), v.trim()),
            None => (spec.trim(), "*"),
        };
        if module.is_empty() {
            return Err(CatalogError::BadRequirement {
                spec: spec.to_string(),
                reason: "missing module name".to_string(),
            });
        }
        Ok(Self {
            module: module.to_string(),
            version: VersionReq::parse(version).map_err(|e| CatalogError::BadRequirement {
                spec: spec.to_string(),
                reason: e.to_string(),
            })?,
            schema: None,
        })
    }
}

impl<'de> Deserialize<'de> for Requirement {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Short(String),
            Table {
                module: String,
                #[serde(default)]
                version: Option<String>,
                #[serde(default)]
                schema: Option<String>,
            },
        }

        match Raw::deserialize(deserializer)? {
            Raw::Short(spec) => Requirement::parse_short(&spec).map_err(serde::de::Error::custom),
            Raw::Table {
                module,
                version,
                schema,
            } => {
                let version = version.unwrap_or_else(|| "*".to_string());
                Ok(Requirement {
                    module,
                    version: VersionReq::parse(&version).map_err(serde::de::Error::custom)?,
                    schema,
                })
            }
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModuleManifest {
    pub name: String,
    pub version: Version,

    #[serde(default)]
    pub description: Option<String>,

    #[serde(default)]
    pub scope: ModuleScope,

    /// Schema used when an install request does not name one. Falls back to the
    /// module name for schema-scoped modules, `public` for database-scoped ones.
    #[serde(default)]
    pub default_schema: Option<String>,

    #[serde(default)]
    pub requires: Vec<Requirement>,

    /// Default `${var}` values for this module's migrations.
    #[serde(default)]
    pub variables: BTreeMap<String, String>,
}

impl ModuleManifest {
    pub fn fallback_schema(&self) -> String {
        self.default_schema
            .clone()
            .unwrap_or_else(|| match self.scope {
                ModuleScope::Schema => self.name.clone(),
                ModuleScope::Database => "public".to_string(),
            })
    }
}

#[derive(Debug, Clone)]
pub struct CatalogModule {
    pub manifest: ModuleManifest,
    pub migrations: Vec<Migration>,
    pub dir: PathBuf,
}

impl CatalogModule {
    pub fn name(&self) -> &str {
        &self.manifest.name
    }

    pub fn version(&self) -> &Version {
        &self.manifest.version
    }

    /// Migrations introduced at or before `version`, in catalog order.
    pub fn migrations_up_to(&self, version: &Version) -> Vec<&Migration> {
        self.migrations
            .iter()
            .filter(|m| &migration_since(m) <= version)
            .collect()
    }
}

/// The version a migration was introduced in; untagged migrations are always
/// applied.
pub fn migration_since(migration: &Migration) -> Version {
    migration
        .since
        .as_deref()
        .and_then(|s| Version::parse(s).ok())
        .unwrap_or_else(|| Version::new(0, 0, 0))
}

#[derive(Debug, Clone, Default)]
pub struct Catalog {
    modules: BTreeMap<String, CatalogModule>,
}

impl Catalog {
    pub fn load<P: AsRef<Path>>(root: P) -> Result<Self, CatalogError> {
        let root = root.as_ref();
        if !root.is_dir() {
            return Err(CatalogError::MissingCatalog {
                path: root.to_path_buf(),
            });
        }

        let mut dirs: Vec<PathBuf> = std::fs::read_dir(root)
            .map_err(|e| CatalogError::Io {
                path: root.to_path_buf(),
                source: e,
            })?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.join(MODULE_MANIFEST).is_file())
            .collect();
        dirs.sort();

        let mut modules = BTreeMap::new();
        for dir in dirs {
            let module = Self::load_module(&dir)?;
            let name = module.manifest.name.clone();
            if let Some(previous) = modules.insert(name.clone(), module) {
                return Err(CatalogError::DuplicateModule {
                    name,
                    first: previous.dir,
                    second: dir,
                });
            }
        }

        let catalog = Self { modules };
        catalog.validate()?;
        Ok(catalog)
    }

    fn load_module(dir: &Path) -> Result<CatalogModule, CatalogError> {
        let manifest_path = dir.join(MODULE_MANIFEST);
        let raw = std::fs::read_to_string(&manifest_path).map_err(|e| CatalogError::Io {
            path: manifest_path.clone(),
            source: e,
        })?;
        let manifest: ModuleManifest =
            toml::from_str(&raw).map_err(|e| CatalogError::BadManifest {
                path: manifest_path.clone(),
                reason: e.to_string(),
            })?;

        let migrations = MigrationLoader::load_from_dir_excluding(dir, &[MODULE_MANIFEST])
            .map_err(|e| CatalogError::BadMigrations {
                module: manifest.name.clone(),
                reason: e.to_string(),
            })?;

        Ok(CatalogModule {
            manifest,
            migrations,
            dir: dir.to_path_buf(),
        })
    }

    fn validate(&self) -> Result<(), CatalogError> {
        for module in self.modules.values() {
            let mut seen = BTreeMap::new();
            for migration in &module.migrations {
                if seen.insert(migration.id.clone(), ()).is_some() {
                    return Err(CatalogError::DuplicateMigrationId {
                        module: module.name().to_string(),
                        id: migration.id.clone(),
                    });
                }

                if let Some(since) = &migration.since {
                    let since =
                        Version::parse(since).map_err(|e| CatalogError::BadMigrationVersion {
                            module: module.name().to_string(),
                            id: migration.id.clone(),
                            reason: e.to_string(),
                        })?;
                    if &since > module.version() {
                        return Err(CatalogError::MigrationAheadOfModule {
                            module: module.name().to_string(),
                            id: migration.id.clone(),
                            since: since.to_string(),
                            module_version: module.version().to_string(),
                        });
                    }
                }
            }

            for requirement in &module.manifest.requires {
                let Some(dependency) = self.modules.get(&requirement.module) else {
                    return Err(CatalogError::UnknownModule {
                        name: requirement.module.clone(),
                        required_by: Some(module.name().to_string()),
                    });
                };
                if !requirement.version.matches(dependency.version()) {
                    return Err(CatalogError::UnsatisfiedRequirement {
                        module: requirement.module.clone(),
                        required_by: module.name().to_string(),
                        wanted: requirement.version.to_string(),
                        available: dependency.version().to_string(),
                    });
                }
            }
        }
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&CatalogModule> {
        self.modules.get(name)
    }

    pub fn modules(&self) -> impl Iterator<Item = &CatalogModule> {
        self.modules.values()
    }

    pub fn len(&self) -> usize {
        self.modules.len()
    }

    pub fn is_empty(&self) -> bool {
        self.modules.is_empty()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CatalogError {
    #[error("migration catalog not found at {path}")]
    MissingCatalog { path: PathBuf },

    #[error("failed to read {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("invalid module manifest {path}: {reason}")]
    BadManifest { path: PathBuf, reason: String },

    #[error("invalid migrations in module '{module}': {reason}")]
    BadMigrations { module: String, reason: String },

    #[error("invalid requirement '{spec}': {reason}")]
    BadRequirement { spec: String, reason: String },

    #[error("module '{name}' defined twice: {first} and {second}", first = first.display(), second = second.display())]
    DuplicateModule {
        name: String,
        first: PathBuf,
        second: PathBuf,
    },

    #[error("duplicate migration id '{id}' in module '{module}'")]
    DuplicateMigrationId { module: String, id: String },

    #[error("migration '{id}' in module '{module}' has an invalid `since`: {reason}")]
    BadMigrationVersion {
        module: String,
        id: String,
        reason: String,
    },

    #[error(
        "migration '{id}' in module '{module}' is tagged since {since}, ahead of the module version {module_version}"
    )]
    MigrationAheadOfModule {
        module: String,
        id: String,
        since: String,
        module_version: String,
    },

    #[error("unknown module '{name}'{}", match required_by { Some(m) => format!(" required by '{m}'"), None => String::new() })]
    UnknownModule {
        name: String,
        required_by: Option<String>,
    },

    #[error(
        "'{required_by}' requires {module} {wanted}, but the catalog provides {module} {available}"
    )]
    UnsatisfiedRequirement {
        module: String,
        required_by: String,
        wanted: String,
        available: String,
    },
}
