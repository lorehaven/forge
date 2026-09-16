//! Lists realm usernames for the assignee picker, reading `auth.users`
//! directly since `UserDb` has no "list everyone" method.

use crate::domain::db::{WorkbenchError, pool};
use quench_auth::prelude::realm;
use quench_db::prelude::Db;
use sqlx::Row;

/// Just enough of `auth.users` for the assignee picker.
pub struct RealmUser {
    pub username: String,
    pub display_name: Option<String>,
}

impl RealmUser {
    /// Display name if set, else the username - never both.
    pub fn label(&self) -> &str {
        self.display_name
            .as_deref()
            .filter(|name| !name.is_empty())
            .unwrap_or(&self.username)
    }
}

pub async fn list_users(db: &Db) -> Result<Vec<RealmUser>, WorkbenchError> {
    let pool = pool(db)?;
    let auth_schema = realm::auth_schema();
    let sql = format!("SELECT username, display_name FROM {auth_schema}.users ORDER BY username");

    let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .fetch_all(pool)
        .await?;

    rows.iter()
        .map(|row| {
            Ok(RealmUser {
                username: row.try_get::<String, _>("username")?,
                display_name: row.try_get::<Option<String>, _>("display_name")?,
            })
        })
        .collect()
}
