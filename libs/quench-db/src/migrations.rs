use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::path::Path;
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Migration {
    pub id: String,
    pub author: String,
    pub changes: Vec<ChangeSet>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationFile {
    pub migrations: Vec<Migration>,
}

pub struct MigrationLoader;

impl MigrationLoader {
    pub fn load_from_dir<P: AsRef<Path>>(dir: P) -> anyhow::Result<Vec<Migration>> {
        let mut files: Vec<_> = WalkDir::new(dir)
            .max_depth(1)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "toml"))
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
