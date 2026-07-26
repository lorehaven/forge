use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt::Debug;
use std::path::Path;
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Migration {
    pub id: String,
    pub author: String,

    /// Module version this migration was introduced in. Migrations are applied
    /// only when `since <= ` the version being installed. Defaults to `0.0.0`,
    /// i.e. always applied.
    #[serde(default)]
    pub since: Option<String>,

    pub changes: Vec<ChangeSet>,
}

impl Migration {
    /// Renders every change to SQL, substituting `${var}` placeholders.
    pub fn to_sql_with(&self, vars: &BTreeMap<String, String>) -> Result<Vec<String>, RenderError> {
        self.changes.iter().map(|c| c.to_sql_with(vars)).collect()
    }
}

/// Substitutes `${name}` placeholders from `vars`.
///
/// Unknown placeholders are an error rather than being left in place - an
/// unresolved variable in a DDL statement is never what the author meant.
pub fn render(template: &str, vars: &BTreeMap<String, String>) -> Result<String, RenderError> {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;

    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let tail = &rest[start + 2..];
        let Some(end) = tail.find('}') else {
            return Err(RenderError::Unterminated {
                template: template.to_string(),
            });
        };
        let name = tail[..end].trim();
        let value = vars.get(name).ok_or_else(|| RenderError::UnknownVariable {
            name: name.to_string(),
            known: vars.keys().cloned().collect(),
        })?;
        out.push_str(value);
        rest = &tail[end + 1..];
    }

    out.push_str(rest);
    Ok(out)
}

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("unknown variable '{name}' (known: {})", known.join(", "))]
    UnknownVariable { name: String, known: Vec<String> },

    #[error("unterminated '${{' in template: {template}")]
    Unterminated { template: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationFile {
    pub migrations: Vec<Migration>,
}

pub struct MigrationLoader;

impl MigrationLoader {
    pub fn load_from_dir<P: AsRef<Path>>(dir: P) -> anyhow::Result<Vec<Migration>> {
        Self::load_from_dir_excluding(dir, &[])
    }

    /// Same as [`MigrationLoader::load_from_dir`], but skips the named files.
    /// Used by the catalog to ignore `module.toml` sitting next to migrations.
    pub fn load_from_dir_excluding<P: AsRef<Path>>(
        dir: P,
        exclude: &[&str],
    ) -> anyhow::Result<Vec<Migration>> {
        let mut files: Vec<_> = WalkDir::new(dir)
            .max_depth(1)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "toml"))
            .filter(|e| {
                !e.file_name()
                    .to_str()
                    .is_some_and(|name| exclude.contains(&name))
            })
            .collect();

        // Sort files by name (yyyy-mm-dd-0000.toml)
        files.sort_by(|a, b| a.file_name().cmp(b.file_name()));

        let mut all_migrations = Vec::new();

        for entry in files {
            let content = std::fs::read_to_string(entry.path())?;
            let mut file: MigrationFile = toml::from_str(&content)?;

            // Sort migrations inside the file by ID (0000-text)
            file.migrations.sort_by(|a, b| a.id.cmp(&b.id));

            // Validate uniqueness of 0000- prefixes in this file
            let mut ids = std::collections::HashSet::new();
            for migration in &file.migrations {
                let prefix = migration.id.split('-').next().unwrap_or("");
                if !ids.insert(prefix) {
                    anyhow::bail!(
                        "Duplicate migration prefix '{}' in file {:?}",
                        prefix,
                        entry.path()
                    );
                }
            }

            all_migrations.extend(file.migrations);
        }

        Ok(all_migrations)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChangeSet {
    #[serde(rename = "sql")]
    Sql(String),

    #[serde(rename = "createSchema")]
    CreateSchema { name: String },

    #[serde(rename = "createExtension")]
    CreateExtension { name: String },

    #[serde(rename = "createTable")]
    CreateTable {
        schema: Option<String>,
        name: String,
        columns: Vec<ColumnDef>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnDef {
    pub name: String,
    pub data_type: String,
    pub primary_key: Option<bool>,
    pub nullable: Option<bool>,
}

impl ChangeSet {
    /// Renders this change to SQL with `${var}` placeholders substituted.
    pub fn to_sql_with(&self, vars: &BTreeMap<String, String>) -> Result<String, RenderError> {
        render(&self.to_sql(), vars)
    }

    pub fn to_sql(&self) -> String {
        match self {
            ChangeSet::Sql(sql) => sql.clone(),

            ChangeSet::CreateSchema { name } => {
                format!("CREATE SCHEMA IF NOT EXISTS {}", name)
            }

            ChangeSet::CreateExtension { name } => {
                format!("CREATE EXTENSION IF NOT EXISTS {}", name)
            }

            ChangeSet::CreateTable {
                schema,
                name,
                columns,
            } => {
                let full_name = if let Some(s) = schema {
                    format!("{}.{}", s, name)
                } else {
                    name.clone()
                };
                let col_defs: Vec<String> = columns
                    .iter()
                    .map(|c| {
                        let mut def = format!("{} {}", c.name, c.data_type);
                        if c.primary_key.unwrap_or(false) {
                            def.push_str(" PRIMARY KEY");
                        }
                        if !c.nullable.unwrap_or(true) {
                            def.push_str(" NOT NULL");
                        }
                        def
                    })
                    .collect();
                format!(
                    "CREATE TABLE IF NOT EXISTS {} ({})",
                    full_name,
                    col_defs.join(", ")
                )
            }
        }
    }
}
