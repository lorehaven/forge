//! Turns "install these modules at these versions" into an ordered migration
//! plan: resolve the dependency graph, topologically sort it, gate migrations
//! by version and render their SQL.

use crate::catalog::{Catalog, CatalogError, ModuleScope, migration_since};
use crate::migrations::{Migration, RenderError};
use semver::Version;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, HashMap};

/// Schema placeholder used as the instance key of database-scoped modules, so
/// they resolve to a single instance no matter how many schemas pull them in.
const DATABASE_SCOPE_KEY: &str = "*";

/// A module the operator asked for.
#[derive(Debug, Clone, Default)]
pub struct InstallRequest {
    pub module: String,
    /// Version to install. Defaults to the version the catalog defines.
    pub version: Option<Version>,
    /// Target schema. Defaults to the module's `default_schema`.
    pub schema: Option<String>,
    /// Overrides for the module's `${var}` defaults.
    pub variables: BTreeMap<String, String>,
}

impl InstallRequest {
    pub fn new(module: impl Into<String>) -> Self {
        Self {
            module: module.into(),
            ..Default::default()
        }
    }

    /// Parses `module[@version][:schema]`, e.g. `sage@0.1.9:sage_test`.
    pub fn parse(spec: &str) -> Result<Self, PlanError> {
        let (head, schema) = match spec.split_once(':') {
            Some((head, schema)) => (head, Some(schema.trim().to_string())),
            None => (spec, None),
        };
        let (module, version) = match head.split_once('@') {
            Some((module, version)) => (module.trim(), Some(version.trim())),
            None => (head.trim(), None),
        };

        if module.is_empty() {
            return Err(PlanError::BadInstallSpec {
                spec: spec.to_string(),
                reason: "missing module name".to_string(),
            });
        }

        let version = version
            .map(|v| {
                Version::parse(v).map_err(|e| PlanError::BadInstallSpec {
                    spec: spec.to_string(),
                    reason: e.to_string(),
                })
            })
            .transpose()?;

        Ok(Self {
            module: module.to_string(),
            version,
            schema: schema.filter(|s| !s.is_empty()),
            variables: BTreeMap::new(),
        })
    }
}

/// One module installed into one schema.
#[derive(Debug, Clone)]
pub struct ResolvedModule {
    pub module: String,
    pub version: Version,
    /// Schema the migrations run against.
    pub schema: String,
    pub scope: ModuleScope,
    /// Set when the module was pulled in as a dependency.
    pub required_by: Option<String>,
    pub variables: BTreeMap<String, String>,
}

impl ResolvedModule {
    /// Stable identity of this module instance, used in the ledger.
    pub fn instance(&self) -> String {
        instance_key(&self.module, &self.schema, self.scope)
    }
}

fn instance_key(module: &str, schema: &str, scope: ModuleScope) -> String {
    match scope {
        ModuleScope::Schema => format!("{module}@{schema}"),
        ModuleScope::Database => format!("{module}@{DATABASE_SCOPE_KEY}"),
    }
}

/// A single migration, resolved and rendered, ready to be applied.
#[derive(Debug, Clone)]
pub struct PlannedMigration {
    /// Ledger primary key: `module@schema:migration-id`.
    pub id: String,
    pub module: String,
    pub schema: String,
    pub migration_id: String,
    pub author: String,
    /// Module version this migration was introduced in.
    pub since: Version,
    /// Version the module is being installed at.
    pub module_version: Version,
    pub statements: Vec<String>,
    pub checksum: String,
}

#[derive(Debug, Clone, Default)]
pub struct MigrationPlan {
    /// Modules in dependency order.
    pub modules: Vec<ResolvedModule>,
    /// Migrations in the order they must be applied.
    pub migrations: Vec<PlannedMigration>,
}

impl MigrationPlan {
    pub fn is_empty(&self) -> bool {
        self.migrations.is_empty()
    }

    /// Builds the plan for `requests` against `catalog`.
    pub fn resolve(catalog: &Catalog, requests: &[InstallRequest]) -> Result<Self, PlanError> {
        let modules = resolve_modules(catalog, requests)?;
        let mut migrations = Vec::new();

        for resolved in &modules {
            let module = catalog.get(&resolved.module).ok_or_else(|| {
                PlanError::Catalog(CatalogError::UnknownModule {
                    name: resolved.module.clone(),
                    required_by: resolved.required_by.clone(),
                })
            })?;

            let mut vars = module.manifest.variables.clone();
            vars.extend(resolved.variables.clone());
            vars.insert("schema".to_string(), resolved.schema.clone());
            vars.insert("module".to_string(), resolved.module.clone());
            vars.insert("version".to_string(), resolved.version.to_string());

            for migration in module.migrations_up_to(&resolved.version) {
                migrations.push(plan_migration(resolved, migration, &vars)?);
            }
        }

        Ok(Self {
            modules,
            migrations,
        })
    }
}

fn plan_migration(
    resolved: &ResolvedModule,
    migration: &Migration,
    vars: &BTreeMap<String, String>,
) -> Result<PlannedMigration, PlanError> {
    let statements = migration
        .to_sql_with(vars)
        .map_err(|source| PlanError::Render {
            module: resolved.module.clone(),
            migration: migration.id.clone(),
            source,
        })?;

    let mut hasher = Sha256::new();
    for statement in &statements {
        hasher.update(statement.as_bytes());
        hasher.update(b";\n");
    }
    let checksum = hex::encode(hasher.finalize());

    Ok(PlannedMigration {
        id: format!("{}:{}", resolved.instance(), migration.id),
        module: resolved.module.clone(),
        schema: resolved.schema.clone(),
        migration_id: migration.id.clone(),
        author: migration.author.clone(),
        since: migration_since(migration),
        module_version: resolved.version.clone(),
        statements,
        checksum,
    })
}

/// An edge in the resolved graph, kept so requirements can be validated once
/// every instance version is known.
struct Edge {
    from: String,
    to: String,
    wanted: semver::VersionReq,
}

fn resolve_modules(
    catalog: &Catalog,
    requests: &[InstallRequest],
) -> Result<Vec<ResolvedModule>, PlanError> {
    if requests.is_empty() {
        return Err(PlanError::NothingToInstall);
    }

    let mut instances: BTreeMap<String, ResolvedModule> = BTreeMap::new();
    let mut pinned: BTreeSet<String> = BTreeSet::new();
    let mut edges: Vec<Edge> = Vec::new();

    // Explicit requests first: they pin versions that dependencies must satisfy.
    for request in requests {
        let module = catalog.get(&request.module).ok_or_else(|| {
            PlanError::Catalog(CatalogError::UnknownModule {
                name: request.module.clone(),
                required_by: None,
            })
        })?;

        let version = request
            .version
            .clone()
            .unwrap_or_else(|| module.version().clone());
        if &version > module.version() {
            return Err(PlanError::VersionNotInCatalog {
                module: request.module.clone(),
                wanted: version.to_string(),
                available: module.version().to_string(),
            });
        }

        let schema = request
            .schema
            .clone()
            .unwrap_or_else(|| module.manifest.fallback_schema());
        let key = instance_key(&request.module, &schema, module.manifest.scope);

        if let Some(existing) = instances.get(&key)
            && existing.version != version
        {
            return Err(PlanError::ConflictingVersions {
                module: request.module.clone(),
                schema,
                first: existing.version.to_string(),
                second: version.to_string(),
            });
        }

        pinned.insert(key.clone());
        instances.insert(
            key,
            ResolvedModule {
                module: request.module.clone(),
                version,
                schema,
                scope: module.manifest.scope,
                required_by: None,
                variables: request.variables.clone(),
            },
        );
    }

    // Walk requirements breadth-first, adding instances as they are discovered.
    let mut queue: Vec<String> = instances.keys().cloned().collect();
    let mut visited: BTreeSet<String> = BTreeSet::new();

    while let Some(key) = queue.pop() {
        if !visited.insert(key.clone()) {
            continue;
        }

        let current = instances[&key].clone();
        let module = catalog.get(&current.module).ok_or_else(|| {
            PlanError::Catalog(CatalogError::UnknownModule {
                name: current.module.clone(),
                required_by: current.required_by.clone(),
            })
        })?;

        for requirement in &module.manifest.requires {
            let dependency = catalog.get(&requirement.module).ok_or_else(|| {
                PlanError::Catalog(CatalogError::UnknownModule {
                    name: requirement.module.clone(),
                    required_by: Some(current.module.clone()),
                })
            })?;

            let schema = match dependency.manifest.scope {
                ModuleScope::Database => dependency.manifest.fallback_schema(),
                ModuleScope::Schema => requirement
                    .schema
                    .clone()
                    .unwrap_or_else(|| current.schema.clone()),
            };
            let dependency_key =
                instance_key(&requirement.module, &schema, dependency.manifest.scope);

            instances
                .entry(dependency_key.clone())
                .or_insert_with(|| ResolvedModule {
                    module: requirement.module.clone(),
                    version: dependency.version().clone(),
                    schema,
                    scope: dependency.manifest.scope,
                    required_by: Some(current.module.clone()),
                    variables: BTreeMap::new(),
                });

            edges.push(Edge {
                from: key.clone(),
                to: dependency_key.clone(),
                wanted: requirement.version.clone(),
            });
            queue.push(dependency_key);
        }
    }

    // Every version is known now, so requirements can be checked for real.
    for edge in &edges {
        let dependency = &instances[&edge.to];
        if !edge.wanted.matches(&dependency.version) {
            let dependent = &instances[&edge.from];
            return Err(PlanError::Catalog(CatalogError::UnsatisfiedRequirement {
                module: dependency.module.clone(),
                required_by: dependent.module.clone(),
                wanted: edge.wanted.to_string(),
                available: dependency.version.to_string(),
            }));
        }
    }

    topological_order(instances, &edges)
}

/// Kahn's algorithm, dependencies first, ties broken by instance key so the
/// plan is byte-for-byte reproducible.
fn topological_order(
    instances: BTreeMap<String, ResolvedModule>,
    edges: &[Edge],
) -> Result<Vec<ResolvedModule>, PlanError> {
    // dependency -> dependents, and how many dependencies each node still has.
    let mut dependents: HashMap<&str, BTreeSet<&str>> = HashMap::new();
    let mut remaining: HashMap<&str, BTreeSet<&str>> = HashMap::new();

    for key in instances.keys() {
        dependents.entry(key.as_str()).or_default();
        remaining.entry(key.as_str()).or_default();
    }
    for edge in edges {
        // Self-dependencies carry no ordering information.
        if edge.from == edge.to {
            continue;
        }
        dependents
            .entry(edge.to.as_str())
            .or_default()
            .insert(edge.from.as_str());
        remaining
            .entry(edge.from.as_str())
            .or_default()
            .insert(edge.to.as_str());
    }

    let mut ready: BinaryHeap<std::cmp::Reverse<&str>> = instances
        .keys()
        .filter(|key| remaining[key.as_str()].is_empty())
        .map(|key| std::cmp::Reverse(key.as_str()))
        .collect();

    let mut ordered: Vec<&str> = Vec::with_capacity(instances.len());
    while let Some(std::cmp::Reverse(key)) = ready.pop() {
        ordered.push(key);
        for dependent in dependents[key].clone() {
            let pending = remaining.get_mut(dependent).expect("known instance");
            pending.remove(key);
            if pending.is_empty() {
                ready.push(std::cmp::Reverse(dependent));
            }
        }
    }

    if ordered.len() != instances.len() {
        let mut cycle: Vec<String> = instances
            .keys()
            .filter(|key| !ordered.contains(&key.as_str()))
            .cloned()
            .collect();
        cycle.sort();
        return Err(PlanError::DependencyCycle { modules: cycle });
    }

    let order: Vec<String> = ordered.into_iter().map(str::to_string).collect();
    let mut instances = instances;
    Ok(order
        .into_iter()
        .map(|key| instances.remove(&key).expect("known instance"))
        .collect())
}

#[derive(Debug, thiserror::Error)]
pub enum PlanError {
    #[error("no modules requested")]
    NothingToInstall,

    #[error("invalid install spec '{spec}': {reason}")]
    BadInstallSpec { spec: String, reason: String },

    #[error("{0}")]
    Catalog(#[from] CatalogError),

    #[error("module '{module}' {wanted} requested, but the catalog only defines up to {available}")]
    VersionNotInCatalog {
        module: String,
        wanted: String,
        available: String,
    },

    #[error("module '{module}' requested twice for schema '{schema}' at {first} and {second}")]
    ConflictingVersions {
        module: String,
        schema: String,
        first: String,
        second: String,
    },

    #[error("dependency cycle between: {}", modules.join(", "))]
    DependencyCycle { modules: Vec<String> },

    #[error("failed to render migration '{migration}' of module '{module}': {source}")]
    Render {
        module: String,
        migration: String,
        #[source]
        source: RenderError,
    },
}
