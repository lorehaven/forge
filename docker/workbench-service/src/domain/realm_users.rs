//! Listing realm usernames, for the assignee picker.
//!
//! Workbench owns no user table of its own - identity is federated through
//! gatehouse's realm (`auth.users`), the same table `issues.assignee` and
//! `issues.reporter` already reference as a foreign key. `quench-auth`'s own
//! `UserDb` has no "list everyone" method, only a lookup by username, so this
//! reads the shared table directly rather than adding new API surface to a
//! library every other service also depends on, for the sake of one picker.

use crate::domain::db::{WorkbenchError, pool};
use quench_auth::prelude::realm;
use quench_db::prelude::Db;
use sqlx::Row;

/// Just enough of `auth.users` for the assignee picker: the username (the
/// actual FK value `issues.assignee` stores) and the display name shown for
/// it, when the realm user has set one.
pub struct RealmUser {
    pub username: String,
    pub display_name: Option<String>,
}

impl RealmUser {
    /// What the picker shows: the display name if set, the username
    /// otherwise - never both, so a user who never set one does not see
    /// their own username repeated.
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
