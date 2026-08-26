//! The APK catalog: one row per published version, keyed by
//! `<package_name>@<version_code>`.
//!
//! Unlike the dynamic-storage tables in [`super::storage`] and
//! [`super::storage_file`], nothing here needs a locked read-then-write - an
//! APK version is immutable once published (yanking flips a flag, it doesn't
//! rewrite content) - so this goes through `quench-db`'s generic
//! [`quench_db::prelude::Crud`] via a [`quench_db::prelude::Repository`]
//! instead of hand-written SQL, the same way
//! `switchboard-service`'s `ModelStore` does for its own simple tables.
//!
//! `created_at` and `yanked` are set by the caller rather than left to the
//! column defaults: `Crud::create` populates every column from the model's
//! serialized JSON (`jsonb_populate_record`), and a key that JSON omits or
//! sets to `null` overrides a `DEFAULT` with `NULL` rather than leaving it
//! alone - so a field with a `NOT NULL` column has to be filled in here, not
//! trusted to Postgres.

use chrono::{DateTime, Utc};
use quench_db::prelude::Model;
use sqlx::types::Json;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
pub struct ApkVersion {
    pub id: String,
    pub package_name: String,
    pub version_code: i64,
    pub version_name: String,
    pub min_sdk_version: Option<i32>,
    pub target_sdk_version: Option<i32>,
    pub label: Option<String>,
    pub permissions: Json<Vec<String>>,
    pub size_bytes: i64,
    pub sha256: String,
    pub uploaded_by: String,
    pub yanked: bool,
    pub created_at: DateTime<Utc>,
}

impl ApkVersion {
    /// The catalog key a caller addresses a version by.
    pub fn id_for(package_name: &str, version_code: i64) -> String {
        format!("{package_name}@{version_code}")
    }
}

impl Model for ApkVersion {
    fn table_name() -> String {
        format!("{}.apk_versions", crate::domain::db::schema())
    }

    fn columns() -> Vec<&'static str> {
        vec![
            "id",
            "package_name",
            "version_code",
            "version_name",
            "min_sdk_version",
            "target_sdk_version",
            "label",
            "permissions",
            "size_bytes",
            "sha256",
            "uploaded_by",
            "yanked",
            "created_at",
        ]
    }
}
