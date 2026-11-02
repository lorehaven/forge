//! The nightly run: a saved schedule (time, zone, stages) and the loop that starts it.
//!
//! The schedule is one JSON value in the index's `meta` table. A loop checks it every half minute and starts the stages
//! as the one job when the local time has reached the set time and the schedule has not started today. If another job
//! is running then, it starts as soon as that one is done (the same day). Once a day, however it ended.

use crate::app::App;
use crate::index::{meta_get, meta_set};
use crate::jobs::Log;
use crate::{convert, download, images, similar, sync, youtube};
use quench_core::time::Time;
use quench_serde::json;
use quench_tz::Zone;
use songbook_core::Schedule;
use std::time::Duration;

const KEY: &str = "schedule";

/// `HH:MM` as hour and minute.
#[must_use]
pub fn parse_time(text: &str) -> Option<(u32, u32)> {
    let (h, m) = text.trim().split_once(':')?;
    let (h, m): (u32, u32) = (h.parse().ok()?, m.parse().ok()?);
    (h < 24
        && m < 60
        && h.to_string().len() <= 2
        && m.to_string().len() <= 2
        && text.trim().len() <= 5)
        .then_some((h, m))
}

/// The zone named, or the service's own for an empty name.
pub fn zone_of(name: &str) -> Result<Zone, String> {
    let name = name.trim();
    if name.is_empty() {
        Ok(Zone::local())
    } else {
        Zone::named(name).map_err(|e| e.to_string())
    }
}

fn date(c: &quench_core::time::Civil) -> String {
    format!("{:04}-{:02}-{:02}", c.year, c.month, c.day)
}

fn stamp(zone: &Zone, t: Time) -> String {
    let c = zone.civil_at(t);
    format!("{} {:02}:{:02}", date(&c), c.hour, c.minute)
}

/// Checks what a person sent and normalises it (`HH:MM`, a known zone, at least one stage).
pub fn check(mut s: Schedule) -> Result<Schedule, String> {
    let (h, m) = parse_time(&s.time).ok_or("the time must be HH:MM, 00:00 to 23:59")?;
    s.time = format!("{h:02}:{m:02}");
    s.zone = s.zone.trim().to_string();
    zone_of(&s.zone)?;
    if !(s.sync || s.links || s.download || s.convert || s.suggestions || s.images) {
        return Err("choose at least one stage".into());
    }
    Ok(s)
}

/// Is it time to start? Enabled, the set time has been reached today (in the zone) and it has not started today.
#[must_use]
pub fn due(s: &Schedule, now: Time) -> bool {
    let (Some((h, m)), Ok(zone)) = (parse_time(&s.time), zone_of(&s.zone)) else {
        return false;
    };
    let c = zone.civil_at(now);
    s.enabled && s.last_day != date(&c) && (c.hour, c.minute) >= (h, m)
}

/// When it runs next, as `YYYY-MM-DD HH:MM` in its zone; empty when it is off.
#[must_use]
pub fn next_run(s: &Schedule, now: Time) -> String {
    let (Some((h, m)), Ok(zone)) = (parse_time(&s.time), zone_of(&s.zone)) else {
        return String::new();
    };
    if !s.enabled {
        return String::new();
    }
    let c = zone.civil_at(now);
    let today_open = s.last_day != date(&c);
    // Today if it has not started yet (a time already passed means: as soon as the running job is done), else tomorrow.
    if today_open {
        if (c.hour, c.minute) >= (h, m) {
            return stamp(&zone, now);
        }
        return format!("{} {h:02}:{m:02}", date(&c));
    }
    let tomorrow = zone.civil_at(Time::from_unix(now.timestamp() + 86_400));
    format!("{} {h:02}:{m:02}", date(&tomorrow))
}

/// The saved schedule (the default when none is saved) with the fields the service fills.
pub async fn load(app: &App) -> Result<Schedule, String> {
    let stored = app
        .index
        .run(|db| meta_get(db, KEY))
        .await
        .map_err(|e| e.to_string())?;
    let mut s = stored
        .and_then(|t| json::from_str::<Schedule>(&t).ok())
        .unwrap_or_default();
    s.zone_in_use = zone_of(&s.zone).map_or_else(|_| String::new(), |z| z.name().to_string());
    s.next_run = next_run(&s, Time::now());
    Ok(s)
}

async fn store(app: &App, s: &Schedule) -> Result<(), String> {
    let text = json::to_string(s);
    app.index
        .run(move |db| meta_set(db, KEY, &text))
        .await
        .map_err(|e| e.to_string())
}

/// Saves what a person set, keeping when it last ran. Saving a time that has already passed today waits for tomorrow
/// instead of starting at once ("run now" is for that).
pub async fn save(app: &App, wanted: Schedule) -> Result<Schedule, String> {
    let mut s = check(wanted)?;
    let kept = load(app).await?;
    s.last_day = kept.last_day;
    s.last_run = kept.last_run;
    s.last_result = kept.last_result;
    let now = Time::now();
    if due(&s, now) {
        let zone = zone_of(&s.zone)?;
        s.last_day = date(&zone.civil_at(now));
    }
    store(app, &s).await?;
    load(app).await
}

async fn record(app: &App, result: &str) {
    if let Ok(mut s) = load(app).await {
        s.last_result = result.to_string();
        let _ = store(app, &s).await;
    }
}

fn header(log: &Log, what: &str) {
    log.line(format!("\n== {what} =="));
}

/// Runs the chosen stages in order. A failed stage is logged and the next one still runs; `Err` lists the failed ones.
pub async fn run_stages(app: &App, s: &Schedule, log: &Log) -> Result<(), String> {
    let mut failed = Vec::new();
    let mut note = |name: &str, r: Result<(), String>| {
        if let Err(e) = r {
            log.line(format!("{name} failed: {e}"));
            failed.push(format!("{name}: {e}"));
        }
    };
    if s.sync {
        header(
            log,
            if s.fresh_ratings {
                "sync (fresh ratings)"
            } else {
                "sync"
            },
        );
        note(
            "sync",
            sync::run_with(app, s.singles, s.fresh_ratings, log).await,
        );
    }
    if s.links {
        header(log, "find links (again for songs with no match)");
        note("find links", youtube::run(app, "", None, true, log).await);
    }
    if s.download {
        header(log, "download");
        note("download", download::run(app, "", None, 3, 5, log).await);
    }
    if s.convert {
        header(log, "convert");
        note("convert", convert::run(&app.config.data, true, log).await);
    }
    if s.suggestions {
        header(log, "refresh suggestions");
        note("suggestions", similar::run(app, 50, 20, "", log).await);
    }
    if s.images {
        header(log, "images");
        note("images", images::run(app, false, log).await);
    }
    if failed.is_empty() {
        log.line("\n== scheduled run finished ==");
        Ok(())
    } else {
        Err(failed.join("; "))
    }
}

/// Starts the saved schedule's stages as the job now, whatever the time. `false` when a job is running.
pub async fn start_now(app: &App) -> bool {
    if app.jobs.running() {
        return false;
    }
    let Ok(mut s) = load(app).await else {
        return false;
    };
    if let Ok(zone) = zone_of(&s.zone) {
        s.last_run = stamp(&zone, Time::now());
    }
    s.last_result = "running".into();
    let _ = store(app, &s).await;
    let worker = app.clone();
    app.jobs.start("scheduled", move |log| async move {
        let result = run_stages(&worker, &s, &log).await;
        record(
            &worker,
            result.as_ref().map_or_else(String::as_str, |()| "ok"),
        )
        .await;
        result
    })
}

/// Checks the schedule every half minute for as long as the service runs.
pub fn spawn(app: App) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(30)).await;
            let Ok(mut s) = load(&app).await else {
                continue;
            };
            let now = Time::now();
            if !due(&s, now) || app.jobs.running() {
                continue;
            }
            let Ok(zone) = zone_of(&s.zone) else { continue };
            s.last_day = date(&zone.civil_at(now));
            if store(&app, &s).await.is_err() {
                continue;
            }
            quench_log::info!("schedule: starting the scheduled run");
            if !start_now(&app).await {
                // Another job got in between: the day is not used up.
                s.last_day.clear();
                let _ = store(&app, &s).await;
            }
        }
    });
}
