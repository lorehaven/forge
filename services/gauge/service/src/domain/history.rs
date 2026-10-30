//! A month of history: the live points folded into one-minute samples and kept in Postgres.
//!
//! The in-memory ring (`store`) stays what draws the live charts; this is what answers "yesterday" and "last week".
//! It is optional: with no `DATABASE_URL` Gauge runs as before, and a database that is down only costs history, never
//! a scrape. Writes happen on a task of their own, one small insert per host per minute, and a failed one is logged
//! and dropped.

use super::alert_log::AlertChange;
use super::model::Filesystem;
use super::store::{ForecastCell, Note, Point};
use gauge_core::Range;
use gauge_core::{Alert, AlertEvent, DiskForecast, forecast};
use quench_db::prelude::Db;
use quench_pg::Pool;
use std::fmt::Write as _;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::sync::mpsc::UnboundedReceiver;

/// What a sample holds, in column order. Each has a mean column of this name and a `_max` one.
pub const FIELDS: [&str; 13] = [
    "cpu",
    "mem",
    "net_rx",
    "net_tx",
    "disk_read",
    "disk_write",
    "gpu",
    "vram",
    "psi_cpu",
    "psi_mem",
    "psi_io",
    "disk_util",
    "gpu_power",
];

type Values = [Option<f64>; FIELDS.len()];

const fn values(p: &Point) -> Values {
    [
        p.cpu,
        p.mem,
        p.rx,
        p.tx,
        p.read,
        p.write,
        p.gpu,
        p.vram,
        p.psi_cpu,
        p.psi_mem,
        p.psi_io,
        p.disk_util,
        p.power,
    ]
}

const fn point(t: f64, v: Values) -> Point {
    Point {
        t,
        cpu: v[0],
        mem: v[1],
        rx: v[2],
        tx: v[3],
        read: v[4],
        write: v[5],
        gpu: v[6],
        vram: v[7],
        psi_cpu: v[8],
        psi_mem: v[9],
        psi_io: v[10],
        disk_util: v[11],
        power: v[12],
    }
}

/// One finished minute of one host.
#[derive(Debug, Clone, PartialEq)]
pub struct Bucket {
    /// Unix seconds at the start of the minute.
    pub minute: i64,
    pub scrapes: u32,
    pub mean: Values,
    pub max: Values,
}

#[derive(Debug, Clone, Default)]
struct Minute {
    start: i64,
    scrapes: u32,
    sum: [f64; FIELDS.len()],
    count: [u32; FIELDS.len()],
    max: Values,
}

impl Minute {
    fn finish(self) -> Bucket {
        let mut mean = [None; FIELDS.len()];
        for (i, slot) in mean.iter_mut().enumerate() {
            *slot = (self.count[i] > 0).then(|| self.sum[i] / f64::from(self.count[i]));
        }
        Bucket {
            minute: self.start,
            scrapes: self.scrapes,
            mean,
            max: self.max,
        }
    }
}

/// Folds one host's points into minutes. A minute is handed back once a later one starts (or `close_before` says
/// it is over), so what is stored is never a half-counted minute.
#[derive(Debug, Default)]
pub struct Rollup {
    open: Option<Minute>,
}

fn minute_of(t: f64) -> i64 {
    (t / 60.0).floor() as i64 * 60
}

impl Rollup {
    pub fn push(&mut self, p: &Point) -> Option<Bucket> {
        // A gap says the host was silent; there is nothing in it to average.
        if p.is_gap() {
            return None;
        }
        let start = minute_of(p.t);
        let done = match &self.open {
            Some(open) if open.start != start => self.open.take().map(Minute::finish),
            _ => None,
        };
        let open = self.open.get_or_insert_with(|| Minute {
            start,
            ..Minute::default()
        });
        open.scrapes += 1;
        for (i, v) in values(p).into_iter().enumerate() {
            if let Some(v) = v.filter(|v| v.is_finite()) {
                open.sum[i] += v;
                open.count[i] += 1;
                open.max[i] = Some(open.max[i].map_or(v, |m| m.max(v)));
            }
        }
        done
    }

    /// The open minute, if it started before `now`'s minute: a host that went quiet would otherwise keep its last
    /// minute here until it came back.
    pub fn close_before(&mut self, now: f64) -> Option<Bucket> {
        let over = self.open.as_ref().is_some_and(|o| o.start < minute_of(now));
        over.then(|| self.open.take().map(Minute::finish))?
    }
}

/// How often a host's filesystem row is refreshed, in seconds.
const DISK_EVERY_SECS: f64 = 600.0;

/// How often the forecast behind the "fills soon" alerts is refreshed, in seconds.
const FORECAST_EVERY_SECS: f64 = 600.0;

/// How far back the forecast looks, in days.
const FORECAST_DAYS: f64 = 7.0;

/// The mount point, or the device when the agent leaves it out.
fn mount_of(f: &Filesystem) -> &str {
    f.mount.as_deref().unwrap_or(&f.device)
}

/// How many days are kept: `HISTORY_DAYS`, 30 by default, from 1 to 365.
#[must_use]
pub fn retention_days() -> u64 {
    quench_core::env::get_or("HISTORY_DAYS", "30")
        .parse()
        .map_or(30, |d: u64| d.clamp(1, 365))
}

/// The schema Gauge's tables live in.
fn schema() -> String {
    quench_core::env::get_or("DB_SCHEMA", "gauge")
}

fn insert_sql(schema: &str) -> String {
    let mut columns = String::from("host, t, scrapes");
    let mut marks = String::from("$1, to_timestamp($2::float8), $3");
    for (i, name) in FIELDS.iter().enumerate() {
        let _ = write!(columns, ", {name}, {name}_max");
        let _ = write!(marks, ", ${}, ${}", 4 + 2 * i, 5 + 2 * i);
    }
    // A restart inside a minute writes it again: the first row stays.
    format!(
        "INSERT INTO {schema}.samples ({columns}) VALUES ({marks}) ON CONFLICT (host, t) DO NOTHING"
    )
}

/// A handle to the database the history lives in; empty until it is connected, and for good when there is none.
#[derive(Debug, Clone, Default)]
pub struct History {
    pool: Arc<OnceLock<Pool>>,
}

impl History {
    /// A handle to the database at `url`, connected now (the service connects in the background instead).
    pub async fn connect(url: &str) -> Result<Self, String> {
        match Db::connect(url).await.map_err(|e| e.to_string())? {
            Db::Postgres(db) => {
                let history = Self::default();
                let _ = history.pool.set(db.pool().clone());
                Ok(history)
            }
            Db::InMemory(_) => Err("the database is in memory".into()),
        }
    }

    #[must_use]
    pub fn available(&self) -> bool {
        self.pool.get().is_some()
    }

    /// Stores a finished minute.
    pub async fn write(&self, host: &str, bucket: &Bucket) -> Result<(), String> {
        let pool = self.pool.get().ok_or("no database")?;
        let (minute, scrapes) = (
            bucket.minute as f64,
            i32::try_from(bucket.scrapes).unwrap_or(i32::MAX),
        );
        let mut params: Vec<&dyn quench_pg::ToSql> = vec![&host, &minute, &scrapes];
        for i in 0..FIELDS.len() {
            params.push(&bucket.mean[i]);
            params.push(&bucket.max[i]);
        }
        pool.execute(&insert_sql(&schema()), &params)
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    /// Stores how full each filesystem is, in the hour `at` falls in; the hour's row keeps the latest values.
    pub async fn write_filesystems(
        &self,
        host: &str,
        at: f64,
        filesystems: &[Filesystem],
    ) -> Result<(), String> {
        let pool = self.pool.get().ok_or("no database")?;
        let hour = (at / 3600.0).floor() * 3600.0;
        let sql = format!(
            "INSERT INTO {}.filesystem_samples (host, mount, t, size_bytes, used_bytes) \
             VALUES ($1, $2, to_timestamp($3::float8), $4, $5) \
             ON CONFLICT (host, mount, t) DO UPDATE SET size_bytes = EXCLUDED.size_bytes, used_bytes = EXCLUDED.used_bytes",
            schema()
        );
        for f in filesystems {
            let used = f.size - f.avail;
            pool.execute(&sql, &[&host, &mount_of(f), &hour, &f.size, &used])
                .await
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// Every filesystem with the growth fitted to the last week and the days until it is full.
    pub async fn forecast(&self, now: f64) -> Result<Vec<DiskForecast>, String> {
        let pool = self.pool.get().ok_or("no database")?;
        let since = now - FORECAST_DAYS * 86_400.0;
        let rows = pool
            .query(
                &format!(
                    "SELECT host, mount, extract(epoch FROM t)::float8 AS at, size_bytes, used_bytes \
                     FROM {}.filesystem_samples WHERE t >= to_timestamp($1::float8) ORDER BY host, mount, t",
                    schema()
                ),
                &[&since],
            )
            .await
            .map_err(|e| e.to_string())?;
        let mut out: Vec<DiskForecast> = Vec::new();
        let mut series: Vec<(f64, f64)> = Vec::new();
        let mut key: Option<(String, String)> = None;
        let mut size = 0.0;
        let mut finish =
            |key: &Option<(String, String)>, series: &mut Vec<(f64, f64)>, size: f64| {
                if let Some((host, mount)) = key {
                    let fit = forecast::fit(series, size);
                    out.push(DiskForecast {
                        host: host.clone(),
                        mount: mount.clone(),
                        size,
                        used: series.last().map_or(0.0, |s| s.1),
                        per_day: fit.map(|f| f.per_day),
                        days_left: fit.and_then(|f| f.days_left),
                    });
                }
                series.clear();
            };
        for row in &rows {
            let (host, mount): (String, String) = (
                row.get("host").map_err(|e| e.to_string())?,
                row.get("mount").map_err(|e| e.to_string())?,
            );
            if key.as_ref() != Some(&(host.clone(), mount.clone())) {
                finish(&key, &mut series, size);
                key = Some((host, mount));
            }
            size = row.get("size_bytes").map_err(|e| e.to_string())?;
            series.push((
                row.get("at").map_err(|e| e.to_string())?,
                row.get("used_bytes").map_err(|e| e.to_string())?,
            ));
        }
        finish(&key, &mut series, size);
        Ok(out)
    }

    /// An alert opened. One that is already open for the host and key stays as it is.
    pub async fn alert_opened(&self, host: &str, alert: &Alert, at: f64) -> Result<(), String> {
        let pool = self.pool.get().ok_or("no database")?;
        pool.execute(
            &format!(
                "INSERT INTO {}.alert_events (host, key, kind, detail, severe, opened) \
                 VALUES ($1, $2, $3, $4, $5, to_timestamp($6::float8)) \
                 ON CONFLICT (host, key) WHERE closed IS NULL DO NOTHING",
                schema()
            ),
            &[
                &host,
                &alert.key,
                &alert.kind,
                &alert.detail,
                &alert.severe,
                &at,
            ],
        )
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
    }

    /// An alert is over.
    pub async fn alert_closed(&self, host: &str, key: &str, at: f64) -> Result<(), String> {
        let pool = self.pool.get().ok_or("no database")?;
        pool.execute(
            &format!(
                "UPDATE {}.alert_events SET closed = to_timestamp($3::float8) \
                 WHERE host = $1 AND key = $2 AND closed IS NULL",
                schema()
            ),
            &[&host, &key, &at],
        )
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
    }

    /// Closes what a run before this one left open: nothing here knows if it is still going, and what is will open
    /// again at the next scrape.
    pub async fn close_dangling(&self, at: f64) -> Result<u64, String> {
        let pool = self.pool.get().ok_or("no database")?;
        pool.execute(
            &format!(
                "UPDATE {}.alert_events SET closed = to_timestamp($1::float8) WHERE closed IS NULL",
                schema()
            ),
            &[&at],
        )
        .await
        .map_err(|e| e.to_string())
    }

    /// The alerts that opened or were open within `days`, newest first.
    pub async fn alert_log(
        &self,
        now: f64,
        days: u64,
        limit: i64,
    ) -> Result<Vec<AlertEvent>, String> {
        let pool = self.pool.get().ok_or("no database")?;
        let since = now - days as f64 * 86_400.0;
        let rows = pool
            .query(
                &format!(
                    "SELECT host, key, kind, detail, severe, \
                            extract(epoch FROM opened)::float8 AS o, extract(epoch FROM closed)::float8 AS c \
                     FROM {}.alert_events WHERE COALESCE(closed, opened) >= to_timestamp($1::float8) \
                     ORDER BY opened DESC, id DESC LIMIT $2",
                    schema()
                ),
                &[&since, &limit],
            )
            .await
            .map_err(|e| e.to_string())?;
        let e = |e: quench_pg::Error| e.to_string();
        rows.iter()
            .map(|row| {
                Ok(AlertEvent {
                    host: row.get("host").map_err(e)?,
                    key: row.get("key").map_err(e)?,
                    kind: row.get("kind").map_err(e)?,
                    detail: row.get("detail").map_err(e)?,
                    severe: row.get("severe").map_err(e)?,
                    opened: row.get("o").map_err(e)?,
                    closed: row.get("c").map_err(e)?,
                })
            })
            .collect()
    }

    /// Drops what is older than `days`, from every table.
    pub async fn prune(&self, now: f64, days: u64) -> Result<u64, String> {
        let pool = self.pool.get().ok_or("no database")?;
        let cutoff = now - days as f64 * 86_400.0;
        let mut dropped = 0;
        for (table, column) in [
            ("samples", "t"),
            ("filesystem_samples", "t"),
            ("alert_events", "COALESCE(closed, opened)"),
        ] {
            dropped += pool
                .execute(
                    &format!(
                        "DELETE FROM {}.{table} WHERE {column} < to_timestamp($1::float8)",
                        schema()
                    ),
                    &[&cutoff],
                )
                .await
                .map_err(|e| e.to_string())?;
        }
        Ok(dropped)
    }

    /// The host's points over `range`, `step_secs` apart and ending at the step `now` is in; `max` picks the highest
    /// of each step instead of the mean. A step with no samples is a gap.
    pub async fn read(
        &self,
        host: &str,
        range: Range,
        now: f64,
        max: bool,
    ) -> Result<Vec<Point>, String> {
        let pool = self.pool.get().ok_or("no database")?;
        let step = range.step_secs() as f64;
        let slots = range.window_secs() / range.step_secs();
        let last = (now / step).floor() * step;
        let first = (slots as f64 - 1.0).mul_add(-step, last);
        let mut select = String::new();
        for (i, name) in FIELDS.iter().enumerate() {
            if max {
                let _ = write!(select, ", max({name}_max)::float8 AS v{i}");
            } else {
                let _ = write!(select, ", avg({name})::float8 AS v{i}");
            }
        }
        let sql = format!(
            "SELECT (floor(extract(epoch FROM t)::float8 / $3::float8) * $3::float8)::float8 AS b{select} \
             FROM {}.samples WHERE host = $1 AND t >= to_timestamp($2::float8) GROUP BY b ORDER BY b",
            schema()
        );
        let rows = pool
            .query(&sql, &[&host, &first, &step])
            .await
            .map_err(|e| e.to_string())?;
        let mut found = Vec::with_capacity(rows.len());
        for row in &rows {
            let at: f64 = row.get("b").map_err(|e| e.to_string())?;
            let mut v: Values = [None; FIELDS.len()];
            for (i, slot) in v.iter_mut().enumerate() {
                *slot = row
                    .get(format!("v{i}").as_str())
                    .map_err(|e| e.to_string())?;
            }
            found.push((at, v));
        }
        Ok(grid(first, step, slots, &found))
    }
}

/// `slots` points `step` apart from `first`, each the stored values at that time or a gap.
#[must_use]
pub fn grid(first: f64, step: f64, slots: u64, found: &[(f64, Values)]) -> Vec<Point> {
    let mut found = found.iter().peekable();
    (0..slots)
        .map(|i| {
            let t = (i as f64).mul_add(step, first);
            while found.next_if(|(at, _)| *at < t - step / 2.0).is_some() {}
            match found.next_if(|(at, _)| (*at - t).abs() < step / 2.0) {
                Some((_, v)) => point(t, *v),
                None => Point::gap(t),
            }
        })
        .collect()
}

/// Starts the writer.
///
/// It connects in the background (the database may start after Gauge), then folds the live points into minutes,
/// writes them and prunes once an hour. Without a `DATABASE_URL` it only drains the points.
pub fn spawn(
    history: &History,
    mut points: UnboundedReceiver<Note>,
    url: String,
    days: u64,
    forecast: ForecastCell,
) {
    let history = history.clone();
    tokio::spawn(async move {
        if url.trim().is_empty() {
            quench_log::info!("history: no DATABASE_URL, so no history beyond the last hour");
            while points.recv().await.is_some() {}
            return;
        }
        let pool = connect(&url).await;
        let _ = history.pool.set(pool);
        match history.close_dangling(super::sources::now()).await {
            Ok(0) => {}
            Ok(n) => quench_log::info!("history: closed {n} alerts a previous run left open"),
            Err(e) => quench_log::warn!("history: could not close old alerts: {e}"),
        }
        quench_log::info!("history: keeping {days} days in Postgres");
        keep(&history, &mut points, days, &forecast).await;
    });
}

async fn connect(url: &str) -> Pool {
    loop {
        match Db::connect(url).await {
            Ok(Db::Postgres(db)) => return db.pool().clone(),
            Ok(Db::InMemory(_)) => {
                quench_log::warn!("history: the database is in memory, which keeps nothing");
            }
            Err(e) => quench_log::warn!("history: cannot reach the database ({e}), trying again"),
        }
        tokio::time::sleep(Duration::from_secs(30)).await;
    }
}

async fn keep(
    history: &History,
    points: &mut UnboundedReceiver<Note>,
    days: u64,
    forecast: &ForecastCell,
) {
    let mut rollups: std::collections::HashMap<String, Rollup> = std::collections::HashMap::new();
    let mut disks_written: std::collections::HashMap<String, f64> =
        std::collections::HashMap::new();
    let mut sweep = tokio::time::interval(Duration::from_secs(30));
    let mut last_prune = 0.0;
    let mut last_forecast = 0.0;
    loop {
        let mut done: Vec<(String, Bucket)> = Vec::new();
        tokio::select! {
            got = points.recv() => {
                let Some(note) = got else { return };
                let tick = match note {
                    Note::Tick(tick) => tick,
                    Note::Alert(change) => {
                        write_alert(history, &change).await;
                        continue;
                    }
                };
                let now = tick.point.t;
                // The hour's row is refreshed every ten minutes, so a restart loses little.
                if !tick.filesystems.is_empty()
                    && disks_written.get(&tick.host).is_none_or(|at| now - at >= DISK_EVERY_SECS)
                {
                    disks_written.insert(tick.host.clone(), now);
                    if let Err(e) = history.write_filesystems(&tick.host, now, &tick.filesystems).await {
                        quench_log::warn!("history: {}: could not store the filesystems: {e}", tick.host);
                    }
                }
                if let Some(b) = rollups.entry(tick.host.clone()).or_default().push(&tick.point) {
                    done.push((tick.host, b));
                }
            }
            _ = sweep.tick() => {
                let now = super::sources::now();
                for (host, r) in &mut rollups {
                    if let Some(b) = r.close_before(now) {
                        done.push((host.clone(), b));
                    }
                }
                if now - last_forecast >= FORECAST_EVERY_SECS {
                    last_forecast = now;
                    match history.forecast(now).await {
                        Ok(list) => *forecast.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = list,
                        Err(e) => quench_log::warn!("history: forecast failed: {e}"),
                    }
                }
                if now - last_prune > 3600.0 {
                    last_prune = now;
                    match history.prune(now, days).await {
                        Ok(0) => {}
                        Ok(n) => quench_log::info!("history: dropped {n} samples older than {days} days"),
                        Err(e) => quench_log::warn!("history: pruning failed: {e}"),
                    }
                }
            }
        }
        for (host, bucket) in done {
            if let Err(e) = history.write(&host, &bucket).await {
                quench_log::warn!("history: {host}: could not store a minute: {e}");
            }
        }
    }
}

async fn write_alert(history: &History, change: &AlertChange) {
    let done = match change {
        AlertChange::Opened { host, alert, at } => history.alert_opened(host, alert, *at).await,
        AlertChange::Closed { host, key, at } => history.alert_closed(host, key, *at).await,
    };
    if let Err(e) = done {
        quench_log::warn!("history: could not store an alert change: {e}");
    }
}
