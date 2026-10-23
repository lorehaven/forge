use crate::domain::db::{DaybookError, pool, schema};
use quench_core::time::Time;
use quench_core::uuid::Uuid;
use quench_db::prelude::Db;
use quench_serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub sort_order: i32,
    pub created_at: Time,
}

#[derive(Clone, Debug)]
pub struct NewProject {
    pub name: String,
    pub sort_order: i32,
}

pub async fn list(db: &Db, owner: &str) -> Result<Vec<Project>, DaybookError> {
    let schema = schema();
    let sql = format!(
        "SELECT {COLUMNS} FROM {schema}.projects WHERE owner = $1 ORDER BY sort_order, name"
    );
    let rows = pool(db)?.query(&sql, &[&owner]).await?;
    rows.iter().map(from_row).collect()
}

pub async fn create(db: &Db, owner: &str, new: &NewProject) -> Result<Project, DaybookError> {
    let schema = schema();
    let sql = format!(
        "INSERT INTO {schema}.projects (id, owner, name, sort_order) VALUES ($1, $2, $3, $4) RETURNING {COLUMNS}"
    );
    let row = pool(db)?
        .query_one(
            &sql,
            &[
                &Uuid::new_v4().to_string(),
                &owner,
                &new.name,
                &new.sort_order,
            ],
        )
        .await?;
    from_row(&row)
}

pub async fn update(
    db: &Db,
    owner: &str,
    id: &str,
    new: &NewProject,
) -> Result<Option<Project>, DaybookError> {
    let schema = schema();
    let sql = format!(
        "UPDATE {schema}.projects SET name = $3, sort_order = $4 WHERE id = $1 AND owner = $2 RETURNING {COLUMNS}"
    );
    let row = pool(db)?
        .query_opt(&sql, &[&id, &owner, &new.name, &new.sort_order])
        .await?;
    row.as_ref().map(from_row).transpose()
}

pub async fn delete(db: &Db, owner: &str, id: &str) -> Result<bool, DaybookError> {
    let schema = schema();
    let sql = format!("DELETE FROM {schema}.projects WHERE id = $1 AND owner = $2");
    let result = pool(db)?.execute(&sql, &[&id, &owner]).await?;
    Ok(result > 0)
}

const COLUMNS: &str = "id, name, sort_order, created_at";

fn from_row(row: &quench_pg::Row) -> Result<Project, DaybookError> {
    Ok(Project {
        id: row.get("id")?,
        name: row.get("name")?,
        sort_order: row.get("sort_order")?,
        created_at: row.get("created_at")?,
    })
}
