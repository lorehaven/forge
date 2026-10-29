use crate::domain::db::{DaybookError, pool, schema};
use crate::domain::recurrence::{Mode, Recurrence, Unit};
use quench_core::time::Time;
use quench_core::uuid::Uuid;
use quench_db::prelude::Db;
use quench_pg::PgArray;
use quench_serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct Task {
    pub id: String,
    pub project_id: Option<String>,
    pub content: String,
    pub description: Option<String>,
    pub is_completed: bool,
    pub sort_order: i32,
    pub due_at: Option<Time>,
    /// The due date has no time of day: `due_at` is midnight UTC of the date.
    pub due_all_day: bool,
    pub remind_at: Option<Time>,
    /// The task's labels, ordered by label name.
    pub label_ids: Vec<String>,
    /// Set for a repeating task: completing it rolls `due_at` to the next occurrence instead of closing it.
    pub recurrence: Option<Recurrence>,
    pub created_at: Time,
    pub updated_at: Time,
}

#[derive(Clone, Debug)]
pub struct NewTask {
    pub content: String,
    pub description: Option<String>,
    pub project_id: Option<String>,
    pub sort_order: i32,
    pub due_at: Option<Time>,
    pub due_all_day: bool,
    pub remind_at: Option<Time>,
    pub label_ids: Vec<String>,
    pub recurrence: Option<Recurrence>,
}

#[derive(Clone, Debug)]
pub struct TaskUpdate {
    pub content: String,
    pub description: Option<String>,
    pub project_id: Option<String>,
    pub is_completed: bool,
    pub sort_order: i32,
    pub due_at: Option<Time>,
    pub due_all_day: bool,
    pub remind_at: Option<Time>,
    pub label_ids: Vec<String>,
    pub recurrence: Option<Recurrence>,
}

/// A due date as it is stored: an all-day one is midnight UTC of its date (so every reader sees the same
/// date), and "all day" without a date means nothing.
#[must_use]
pub fn normalize_due(due_at: Option<Time>, all_day: bool) -> (Option<Time>, bool) {
    match (due_at, all_day) {
        (Some(due), true) => (
            Some(Time::from_unix(due.timestamp().div_euclid(86_400) * 86_400)),
            true,
        ),
        (due, _) => (due, false),
    }
}

/// Lists the caller's tasks.
///
/// With no `label_id` that is one list: the inbox when `project_id` is `None`, else that project. With a
/// `label_id` it is every task carrying that label, across the inbox and all projects - narrowed to
/// `project_id` only when one is given. `all` lifts the one-list restriction without needing a label.
pub async fn list(
    db: &Db,
    owner: &str,
    project_id: Option<&str>,
    label_id: Option<&str>,
    all: bool,
    completed: Option<bool>,
) -> Result<Vec<Task>, DaybookError> {
    let schema = schema();
    let sql = format!(
        "SELECT {} FROM {schema}.tasks WHERE owner = $1 \
         AND (($5::bool AND $2::text IS NULL) OR ($4::text IS NOT NULL AND $2::text IS NULL) OR ($2::text IS NULL AND project_id IS NULL) OR project_id = $2) \
         AND ($3::bool IS NULL OR is_completed = $3) \
         AND ($4::text IS NULL OR EXISTS (SELECT 1 FROM {schema}.task_labels tl WHERE tl.task_id = tasks.id AND tl.label_id = $4)) \
         ORDER BY sort_order, created_at",
        columns(&schema)
    );
    let rows = pool(db)?
        .query(&sql, &[&owner, &project_id, &completed, &label_id, &all])
        .await?;
    rows.iter().map(from_row).collect()
}

pub async fn create(db: &Db, owner: &str, new: &NewTask) -> Result<Task, DaybookError> {
    let schema = schema();
    let id = Uuid::new_v4().to_string();
    let pool = pool(db)?;
    let mut conn = pool.get().await?;
    let mut tx = conn.begin().await?;
    let rule = RuleColumns::from(new.recurrence.as_ref());
    let (due_at, due_all_day) = normalize_due(new.due_at, new.due_all_day);
    let sql = format!(
        "INSERT INTO {schema}.tasks (id, owner, project_id, content, description, sort_order, due_at, due_all_day, remind_at, \
         recurrence_unit, recurrence_interval, recurrence_weekdays, recurrence_mode, recurrence_timezone, recurrence_anchor) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)"
    );
    tx.execute(
        &sql,
        &[
            &id,
            &owner,
            &new.project_id,
            &new.content,
            &new.description,
            &new.sort_order,
            &due_at,
            &due_all_day,
            &new.remind_at,
            &rule.unit,
            &rule.interval,
            &rule.weekdays,
            &rule.mode,
            &rule.timezone,
            &rule.present.then_some(due_at).flatten(),
        ],
    )
    .await?;
    set_labels(&mut tx, &schema, owner, &id, &new.label_ids).await?;
    let task = fetch(&mut tx, &schema, owner, &id).await?;
    tx.commit().await?;
    drop(conn);
    task.ok_or(DaybookError::Sql(quench_pg::Error::RowCount {
        expected: "exactly one",
        got: 0,
    }))
}

pub async fn update(
    db: &Db,
    owner: &str,
    id: &str,
    changes: &TaskUpdate,
) -> Result<Option<Task>, DaybookError> {
    update_at(db, owner, id, changes, Time::now()).await
}

/// [`update`] with the clock passed in, for the roll-forward below.
///
/// Completing a task that repeats does not close it: its due date moves to the next occurrence (and its
/// reminder with it, keeping the same lead time) and it stays open. That happens here, in the one write
/// path every client uses, so a client that knows nothing about recurrence still gets a correct series,
/// and two devices completing the same occurrence both land on the same next date.
pub async fn update_at(
    db: &Db,
    owner: &str,
    id: &str,
    changes: &TaskUpdate,
    now: Time,
) -> Result<Option<Task>, DaybookError> {
    let schema = schema();
    let pool = pool(db)?;
    let mut conn = pool.get().await?;
    let mut tx = conn.begin().await?;

    // Locked, so a concurrent completion waits and then sees the rolled row rather than racing it.
    let sql = format!(
        "SELECT is_completed, due_at, due_all_day, recurrence_anchor, {RULE_COLUMNS} FROM {schema}.tasks \
         WHERE id = $1 AND owner = $2 FOR UPDATE"
    );
    let Some(row) = tx.query_opt(&sql, &[&id, &owner]).await? else {
        return Ok(None);
    };
    let was_completed: bool = row.get("is_completed")?;
    let stored_due: Option<Time> = row.get("due_at")?;
    let stored_all_day: bool = row.get("due_all_day")?;
    let stored_anchor: Option<Time> = row.get("recurrence_anchor")?;
    let stored_rule = rule_from_row(&row)?;

    let (requested_due, due_all_day) = normalize_due(changes.due_at, changes.due_all_day);
    let mut due_at = requested_due;
    let mut remind_at = changes.remind_at;
    let mut is_completed = changes.is_completed;

    // Where the schedule is counted from. A user setting the rule or moving the due date restarts it;
    // a roll (below) does not, or "the 31st" would drift to the 28th after the first February.
    let anchor = if changes.recurrence.is_none() {
        None
    } else if stored_rule == changes.recurrence
        && stored_due == requested_due
        && stored_all_day == due_all_day
    {
        stored_anchor.or(requested_due)
    } else {
        requested_due
    };

    if let (true, false, Some(rule), Some(due), Some(anchor)) = (
        changes.is_completed,
        was_completed,
        changes.recurrence.as_ref(),
        requested_due,
        anchor,
    ) && let Some(next) = if due_all_day {
        rule.next_due_all_day(anchor, due, now)
    } else {
        rule.next_due(anchor, due, now)
    } {
        remind_at = remind_at.map(|remind| rule.shift_reminder(remind, due, next, due_all_day));
        due_at = Some(next);
        is_completed = false;
    }

    let rule = RuleColumns::from(changes.recurrence.as_ref());
    let sql = format!(
        "UPDATE {schema}.tasks SET project_id = $3, content = $4, description = $5, is_completed = $6, sort_order = $7, \
         due_at = $8, due_all_day = $16, remind_at = $9, recurrence_unit = $10, recurrence_interval = $11, recurrence_weekdays = $12, \
         recurrence_mode = $13, recurrence_timezone = $14, recurrence_anchor = $15, updated_at = NOW() \
         WHERE id = $1 AND owner = $2"
    );
    tx.execute(
        &sql,
        &[
            &id,
            &owner,
            &changes.project_id,
            &changes.content,
            &changes.description,
            &is_completed,
            &changes.sort_order,
            &due_at,
            &remind_at,
            &rule.unit,
            &rule.interval,
            &rule.weekdays,
            &rule.mode,
            &rule.timezone,
            &anchor,
            &due_all_day,
        ],
    )
    .await?;
    set_labels(&mut tx, &schema, owner, id, &changes.label_ids).await?;
    let task = fetch(&mut tx, &schema, owner, id).await?;
    tx.commit().await?;
    drop(conn);
    Ok(task)
}

pub async fn delete(db: &Db, owner: &str, id: &str) -> Result<bool, DaybookError> {
    let schema = schema();
    let sql = format!("DELETE FROM {schema}.tasks WHERE id = $1 AND owner = $2");
    let result = pool(db)?.execute(&sql, &[&id, &owner]).await?;
    Ok(result > 0)
}

/// Replaces the task's label set. A label that is not the caller's fails the composite foreign key
/// (`23503`), which the router reports as a bad request.
async fn set_labels(
    tx: &mut quench_pg::Transaction<'_>,
    schema: &str,
    owner: &str,
    task_id: &str,
    label_ids: &[String],
) -> Result<(), DaybookError> {
    let sql = format!("DELETE FROM {schema}.task_labels WHERE task_id = $1 AND owner = $2");
    tx.execute(&sql, &[&task_id, &owner]).await?;
    if label_ids.is_empty() {
        return Ok(());
    }
    let mut unique: Vec<&str> = Vec::with_capacity(label_ids.len());
    for id in label_ids {
        if !unique.contains(&id.as_str()) {
            unique.push(id);
        }
    }
    let sql = format!(
        "INSERT INTO {schema}.task_labels (task_id, label_id, owner) SELECT $1, label_id, $2 FROM unnest($3::text[]) AS label_id"
    );
    tx.execute(&sql, &[&task_id, &owner, &PgArray(unique)])
        .await?;
    Ok(())
}

async fn fetch(
    tx: &mut quench_pg::Transaction<'_>,
    schema: &str,
    owner: &str,
    id: &str,
) -> Result<Option<Task>, DaybookError> {
    let sql = format!(
        "SELECT {} FROM {schema}.tasks WHERE id = $1 AND owner = $2",
        columns(schema)
    );
    let row = tx.query_opt(&sql, &[&id, &owner]).await?;
    row.as_ref().map(from_row).transpose()
}

/// The column list, with the task's label ids folded into one array column.
fn columns(schema: &str) -> String {
    format!(
        "id, project_id, content, description, is_completed, sort_order, due_at, due_all_day, remind_at, \
         COALESCE((SELECT array_agg(l.id ORDER BY lower(l.name), l.id) FROM {schema}.task_labels tl \
         JOIN {schema}.labels l ON l.id = tl.label_id WHERE tl.task_id = tasks.id), ARRAY[]::text[]) AS label_ids, \
         {RULE_COLUMNS}, created_at, updated_at"
    )
}

/// The recurrence rule's columns, selected as they are stored.
const RULE_COLUMNS: &str = "recurrence_unit, recurrence_interval, recurrence_weekdays, recurrence_mode, recurrence_timezone";

/// A [`Recurrence`] flattened to the columns it is stored in.
struct RuleColumns {
    present: bool,
    unit: Option<&'static str>,
    interval: Option<i32>,
    weekdays: PgArray<i16>,
    mode: Option<&'static str>,
    timezone: Option<String>,
}

impl From<Option<&Recurrence>> for RuleColumns {
    fn from(rule: Option<&Recurrence>) -> Self {
        Self {
            present: rule.is_some(),
            unit: rule.map(|rule| rule.unit.as_str()),
            interval: rule.and_then(|rule| i32::try_from(rule.interval).ok()),
            weekdays: PgArray(rule.map_or_else(Vec::new, |rule| {
                rule.weekdays.iter().map(|day| i16::from(*day)).collect()
            })),
            mode: rule.map(|rule| rule.mode.as_str()),
            timezone: rule.map(|rule| rule.timezone.clone()),
        }
    }
}

fn rule_from_row(row: &quench_pg::Row) -> Result<Option<Recurrence>, DaybookError> {
    let unit: Option<String> = row.get("recurrence_unit")?;
    let interval: Option<i32> = row.get("recurrence_interval")?;
    let weekdays: Vec<i16> = array(row, "recurrence_weekdays")?;
    let mode: Option<String> = row.get("recurrence_mode")?;
    let timezone: Option<String> = row.get("recurrence_timezone")?;
    let (Some(unit), Some(interval), Some(mode), Some(timezone)) = (unit, interval, mode, timezone)
    else {
        return Ok(None);
    };
    // The table's CHECKs make these unreachable; a row that somehow fails them is read as a plain task.
    let (Some(unit), Some(mode), Ok(interval)) = (
        Unit::parse(&unit),
        Mode::parse(&mode),
        u32::try_from(interval),
    ) else {
        return Ok(None);
    };
    Ok(Some(Recurrence {
        unit,
        interval,
        weekdays: weekdays
            .into_iter()
            .filter_map(|day| u8::try_from(day).ok())
            .collect(),
        mode,
        timezone,
    }))
}

fn from_row(row: &quench_pg::Row) -> Result<Task, DaybookError> {
    Ok(Task {
        id: row.get("id")?,
        project_id: row.get("project_id")?,
        content: row.get("content")?,
        description: row.get("description")?,
        is_completed: row.get("is_completed")?,
        sort_order: row.get("sort_order")?,
        due_at: row.get("due_at")?,
        due_all_day: row.get("due_all_day")?,
        remind_at: row.get("remind_at")?,
        label_ids: array(row, "label_ids")?,
        recurrence: rule_from_row(row)?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

/// A one-dimensional array column, read through its JSON form (`quench-pg` converts arrays to `Value`).
fn array<T: quench_serde::Deserialize>(
    row: &quench_pg::Row,
    column: &str,
) -> Result<Vec<T>, DaybookError> {
    let value: quench_serde::Value = row.get(column)?;
    quench_serde::from_value(&value).map_err(|e| quench_pg::Error::Decode(e.to_string()).into())
}
