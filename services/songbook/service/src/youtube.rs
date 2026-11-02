//! get-urls: find a YouTube video for each song (yt-dlp search, metadata only) and store it in the index (`songs.yt_*`).
//! Also the pieces download shares: running yt-dlp, and its cookies.

use crate::app::App;
use crate::config::Config;
use crate::files;
use crate::index::{browse, int, int_at, opt_int_at, str_at, text};
use crate::jobs::Log;
use quench_serde::{Map, Value, json};
use songbook_core::naming::norm;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::process::Command;
use tokio::sync::{Mutex, OnceCell, Semaphore};

/// What a finished process said.
#[derive(Debug, Clone)]
pub struct Output {
    pub ok: bool,
    pub stdout: String,
    pub stderr: String,
}

/// Runs `program` and waits for it (at most `limit`); the process is killed if it outlives that or this future is dropped.
pub async fn run_tool(program: &str, args: &[String], limit: Duration) -> Result<Output, String> {
    // `yt-dlp` is the copy the service keeps current when it has one (see `ytdlp`).
    let mut command = if program == "yt-dlp" {
        Command::new(crate::ytdlp::program())
    } else {
        Command::new(program)
    };
    if program == "yt-dlp" {
        // Extra options for every yt-dlp run, e.g. `--js-runtimes node` (YouTube's challenges need a JavaScript runtime).
        command.args(quench_core::env::get_or("YTDLP_ARGS", "").split_whitespace());
    }
    command
        .args(args)
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);
    let child = command.output();
    match tokio::time::timeout(limit, child).await {
        Err(_) => Err(format!("{program} did not finish in {}s", limit.as_secs())),
        Ok(Err(e)) => Err(format!("cannot run {program}: {e}")),
        Ok(Ok(out)) => Ok(Output {
            ok: out.status.success(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        }),
    }
}

/// Does yt-dlp say the video needs a login/age check?
#[must_use]
pub fn age_restricted(text: &str) -> bool {
    let t = text.to_lowercase();
    t.contains("confirm your age")
        || t.contains("requires authentication")
        || t.contains("sign in to confirm")
        || {
            // "age.restricted": any one character between the words
            t.match_indices("age").any(|(i, _)| {
                let rest = &t[i + 3..];
                let mut chars = rest.chars();
                chars.next().is_some() && chars.as_str().starts_with("restricted")
            })
        }
}

/// Did YouTube refuse for rate reasons?
#[must_use]
pub fn throttled(text: &str) -> bool {
    text.contains("HTTP Error 403")
        || text.contains("HTTP Error 429")
        || text.to_lowercase().contains("too many requests")
}

/// The cookies yt-dlp should use: a cookies file (checked once against a public video, since expired cookies make every
/// request fail), else a browser's.
#[derive(Debug)]
pub struct Auth {
    file: PathBuf,
    browser: String,
    checked: OnceCell<bool>,
}

impl Auth {
    #[must_use]
    pub fn new(config: &Config) -> Self {
        Self {
            file: config.cookies_file.clone(),
            browser: config.cookies_browser.clone(),
            checked: OnceCell::new(),
        }
    }

    pub async fn args(&self, log: &Log) -> Vec<String> {
        if self.file.is_file() {
            let file = self.file.to_string_lossy().into_owned();
            let ok = *self
                .checked
                .get_or_init(|| async {
                    let probe = ["--cookies", file.as_str(), "--simulate", "--no-warnings", "--skip-download", "https://www.youtube.com/watch?v=dQw4w9WgXcQ"]
                        .map(String::from);
                    match run_tool("yt-dlp", &probe, Duration::from_secs(120)).await {
                        Ok(out) if out.ok => true,
                        Ok(out) => {
                            let why = out.stderr.lines().last().unwrap_or("?").to_string();
                            log.line(format!(
                                "warning: YouTube rejects {file} ({why}); continuing without cookies, so age-restricted videos will fail. Push fresh cookies."
                            ));
                            false
                        }
                        Err(e) => {
                            log.line(format!("warning: cannot check {file}: {e}"));
                            false
                        }
                    }
                })
                .await;
            return if ok {
                vec!["--cookies".into(), file]
            } else {
                Vec::new()
            };
        }
        if self.browser.is_empty() {
            Vec::new()
        } else {
            vec!["--cookies-from-browser".into(), self.browser.clone()]
        }
    }
}

const BAD: [&str; 13] = [
    "live",
    "cover",
    "remix",
    "karaoke",
    "reaction",
    "slowed",
    "sped up",
    "nightcore",
    "8d",
    "instrumental",
    "tutorial",
    "full album",
    "mix",
];

/// Is `word` in `text` as a whole word (or phrase)? Case-insensitive.
#[must_use]
pub fn has_word(text: &str, word: &str) -> bool {
    let t = text.to_lowercase();
    t.match_indices(word).any(|(i, _)| {
        let before = t[..i]
            .chars()
            .next_back()
            .is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
        let after = t[i + word.len()..]
            .chars()
            .next()
            .is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
        before && after
    })
}

fn is_bad(text: &str) -> bool {
    BAD.iter().any(|w| has_word(text, w))
}

/// Scores a yt-dlp search entry. Returns `(score, is_official)`.
#[must_use]
pub fn score_video(v: &Value, artist: &str, title: &str, duration: Option<i64>) -> (f64, bool) {
    let text_of = |k: &str| {
        v.get(k)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let channel = if text_of("channel").is_empty() {
        text_of("uploader")
    } else {
        text_of("channel")
    };
    let video_title = text_of("title");
    let (nch, nart, ntitle) = (norm(&channel), norm(artist), norm(title));
    let verified = v
        .get("channel_is_verified")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let official = nch == format!("{nart}topic")
        || nch.contains("vevo")
        || nch == nart
        || (nart.chars().count() > 3 && nch.contains(&nart))
        || verified;
    let mut score = 0.0;
    if nch == format!("{nart}topic") {
        score += 5.0;
    } else if nch.contains("vevo") || nch == nart {
        score += 4.0;
    } else if official {
        score += 3.0;
    }
    if !ntitle.is_empty() && norm(&video_title).contains(&ntitle) {
        score += 2.0;
    }
    if video_title.to_lowercase().contains("official") {
        score += 1.0;
    }
    if is_bad(&video_title) && !is_bad(title) {
        score -= 3.0;
    }
    let video_duration = v.get("duration").and_then(Value::as_f64);
    if let (Some(expected), Some(actual)) = (
        duration.filter(|d| *d > 0),
        video_duration.filter(|d| *d > 0.0),
    ) {
        let diff = (actual - expected as f64).abs();
        score += if diff <= 3.0 {
            3.0
        } else if diff <= 10.0 {
            1.0
        } else {
            -3.0
        };
    }
    let views = v.get("view_count").and_then(Value::as_f64).unwrap_or(0.0);
    (score + (views / 1e8).min(1.0), official)
}

/// A video found for a song.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub url: String,
    pub channel: String,
    pub title: String,
    pub duration: Option<f64>,
    pub official: bool,
    pub score: f64,
    /// The video's view count, when YouTube said.
    pub views: Option<i64>,
}

impl Candidate {
    fn to_value(&self) -> Value {
        let mut m = Map::new();
        m.insert("url", Value::String(self.url.clone()));
        m.insert("channel", Value::String(self.channel.clone()));
        m.insert("title", Value::String(self.title.clone()));
        m.insert(
            "duration",
            self.duration.map_or(Value::Null, Value::from_f64),
        );
        m.insert("official", Value::from_i64(i64::from(self.official)));
        m.insert("score", Value::from_f64((self.score * 10.0).round() / 10.0));
        Value::Object(m)
    }
}

/// The best of the entries of a search, and whether it is good enough (score of 4 or more).
#[must_use]
pub fn best_of(
    entries: &[Value],
    artist: &str,
    title: &str,
    duration: Option<i64>,
) -> Option<(Candidate, bool)> {
    let (score, official, entry) = entries
        .iter()
        .filter(|e| e.get("id").and_then(Value::as_str).is_some())
        .map(|e| {
            let (s, o) = score_video(e, artist, title, duration);
            (s, o, e)
        })
        .max_by(|a, b| a.0.total_cmp(&b.0))?;
    let text_of = |k: &str| {
        entry
            .get(k)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let channel = if text_of("channel").is_empty() {
        text_of("uploader")
    } else {
        text_of("channel")
    };
    let candidate = Candidate {
        url: format!("https://www.youtube.com/watch?v={}", text_of("id")),
        channel,
        title: text_of("title"),
        duration: entry.get("duration").and_then(Value::as_f64),
        official,
        score,
        views: entry
            .get("view_count")
            .and_then(Value::as_f64)
            .filter(|v| *v >= 0.0)
            .map(|v| v as i64),
    };
    Some((candidate, score >= 4.0))
}

/// Searches YouTube; all searches share one back-off when YouTube throttles.
#[derive(Debug)]
pub struct Search {
    pause_until: Mutex<Instant>,
}

impl Default for Search {
    fn default() -> Self {
        Self {
            pause_until: Mutex::new(Instant::now()),
        }
    }
}

impl Search {
    /// `Ok((best candidate, accepted))`; `Ok(None)` when the search found nothing; `Err` when it could not be done.
    pub async fn find(
        &self,
        auth: &[String],
        artist: &str,
        title: &str,
        duration: Option<i64>,
        log: &Log,
    ) -> Result<Option<(Candidate, bool)>, String> {
        const RETRIES: u32 = 6;
        let query = format!("ytsearch20:{artist} {title}");
        let mut args: Vec<String> = auth.to_vec();
        args.extend(["--flat-playlist", "--no-warnings", "-J"].map(String::from));
        args.push(query);
        let mut stdout = String::new();
        for attempt in 0..RETRIES {
            let wait = self
                .pause_until
                .lock()
                .await
                .saturating_duration_since(Instant::now());
            if !wait.is_zero() {
                tokio::time::sleep(wait).await;
            }
            let out = run_tool("yt-dlp", &args, Duration::from_secs(90)).await?;
            if out.ok {
                stdout = out.stdout;
                break;
            }
            let all = if out.stderr.trim().is_empty() {
                out.stdout.clone()
            } else {
                out.stderr.clone()
            };
            let last = all
                .trim()
                .lines()
                .last()
                .unwrap_or("unknown error")
                .chars()
                .take(200)
                .collect::<String>();
            if age_restricted(&all) {
                return Err(if auth.is_empty() {
                    "YouTube age/authentication restriction. Configure cookies (COOKIES_FILE or COOKIES_BROWSER); the account must be able to view the video.".into()
                } else {
                    "YouTube age/authentication restriction. The configured cookies did not authorize access; refresh the cookies.".into()
                });
            }
            if !throttled(&all) || attempt == RETRIES - 1 {
                return Err(format!("yt-dlp failed: {last}"));
            }
            let jitter = f64::from(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.subsec_millis())
                    % 5000,
            ) / 1000.0;
            let delay = Duration::from_secs_f64(f64::from((15u32 << attempt).min(300)) + jitter);
            {
                let mut until = self.pause_until.lock().await;
                *until = (*until).max(Instant::now() + delay);
            }
            log.line(format!(
                "  throttled by YouTube ({}); pausing all workers {:.0}s (retry {}/{})",
                last.chars().take(60).collect::<String>(),
                delay.as_secs_f64(),
                attempt + 1,
                RETRIES - 1
            ));
        }
        let parsed = if stdout.trim().is_empty() {
            Value::Null
        } else {
            json::parse(&stdout).map_err(|e| e.to_string())?
        };
        let entries: Vec<Value> = parsed
            .get("entries")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter(|e| e.is_object()).cloned().collect())
            .unwrap_or_default();
        Ok(best_of(&entries, artist, title, duration))
    }
}

/// A song to look up: its id, title, file name, duration and main artist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wanted {
    pub id: i64,
    pub title: String,
    pub filename: String,
    pub duration: Option<i64>,
    pub artist: String,
}

/// Songs with no link yet (or, with `retry_missing`, none found last time) and no file in `on_disk`. `artist` limits it to
/// songs by one artist.
pub fn songs_to_look_up(
    db: &mut quench_sqlite::Connection,
    artist: &str,
    retry_missing: bool,
) -> crate::index::Result<Vec<Wanted>> {
    let rows = db.query(
        "SELECT s.id, s.title, s.filename, s.duration, s.yt_status, a.name FROM songs s \
         JOIN song_artist sa ON sa.song_id = s.id AND sa.position = 0 JOIN artists a ON a.id = sa.artist_id \
         WHERE s.yt_manual = 0 ORDER BY s.id",
        &[],
    )?;
    let mine = if artist.is_empty() {
        None
    } else {
        Some(browse::songs_of(db, artist)?)
    };
    Ok(rows
        .rows
        .iter()
        .filter(|r| {
            let status = crate::index::opt_str_at(r, 4);
            status.is_none() || (retry_missing && status.as_deref() == Some("miss"))
        })
        .filter(|r| mine.as_ref().is_none_or(|set| set.contains(&int_at(r, 0))))
        .map(|r| Wanted {
            id: int_at(r, 0),
            title: str_at(r, 1),
            filename: str_at(r, 2),
            duration: opt_int_at(r, 3),
            artist: str_at(r, 5),
        })
        .collect())
}

pub async fn run(
    app: &App,
    artist: &str,
    limit: Option<u32>,
    retry_missing: bool,
    log: &Log,
) -> Result<(), String> {
    crate::ytdlp::ensure(&app.config, log).await;
    let (name, retry) = (artist.to_string(), retry_missing);
    let all = app
        .index
        .run(move |db| songs_to_look_up(db, &name, retry))
        .await
        .map_err(|e| e.to_string())?;
    let on_disk = files::file_index(&app.config.data);
    let total = all.len();
    let mut todo: Vec<Wanted> = all
        .into_iter()
        .filter(|w| !on_disk.contains_key(&w.filename))
        .collect();
    if total > todo.len() {
        log.line(format!(
            "{} songs already downloaded or converted; not looked up",
            total - todo.len()
        ));
    }
    if let Some(n) = limit.filter(|n| *n > 0) {
        todo.truncate(n as usize);
    }
    let linked = app
        .index
        .run(|db| {
            Ok(int_at(
                &db.query("SELECT COUNT(*) FROM songs WHERE yt_url != ''", &[])?
                    .rows[0],
                0,
            ))
        })
        .await
        .map_err(|e| e.to_string())?;
    log.line(format!(
        "{} songs to look up ({linked} already have links; skipped)",
        todo.len()
    ));

    let auth = Arc::new(Auth::new(&app.config));
    let search = Arc::new(Search::default());
    let slots = Arc::new(Semaphore::new(2));
    let jobs: Vec<_> = todo
        .iter()
        .cloned()
        .map(|w| {
            let (auth, search, slots, log) = (
                Arc::clone(&auth),
                Arc::clone(&search),
                Arc::clone(&slots),
                log.clone(),
            );
            tokio::spawn(async move {
                let _slot = slots.acquire().await;
                let args = auth.args(&log).await;
                let found = search
                    .find(&args, &w.artist, &w.title, w.duration, &log)
                    .await;
                (w, found)
            })
        })
        .collect();
    let total = jobs.len();
    for (done, job) in jobs.into_iter().enumerate() {
        let (w, found) = job.await.map_err(|e| e.to_string())?;
        let found = match found {
            Ok(f) => f,
            Err(e) => {
                log.line(format!("  error: {}: {e}", w.title)); // not a miss: tried again next run
                continue;
            }
        };
        let label = found
            .as_ref()
            .filter(|(_, ok)| *ok)
            .map_or_else(|| "no match".to_string(), |(c, _)| c.channel.clone());
        let id = w.id;
        app.index
            .run(move |db| match found {
                Some((c, true)) => db
                    .execute(
                        "UPDATE songs SET yt_url = ?, yt_channel = ?, yt_title = ?, yt_official = ?, yt_score = ?, yt_views = ?, yt_status = 'ok', yt_candidate = '' WHERE id = ?",
                        &[text(&c.url), text(&c.channel), text(&c.title), int(i64::from(c.official)), quench_sqlite::Value::Real((c.score * 10.0).round() / 10.0), c.views.map_or(quench_sqlite::Value::Null, int), int(id)],
                    )
                    .map(|_| ()),
                other => {
                    let cand = other.map_or_else(|| "{}".to_string(), |(c, _)| json::to_string(&c.to_value()));
                    db.execute("UPDATE songs SET yt_status = 'miss', yt_candidate = ? WHERE id = ?", &[text(&cand), int(id)]).map(|_| ())
                }
            })
            .await
            .map_err(|e| e.to_string())?;
        if (done + 1) % 25 == 0 {
            log.line(format!(
                "  {}/{total}  last: {} - {} -> {label}",
                done + 1,
                w.artist,
                w.title
            ));
        }
    }
    write_misses(app, log).await?;
    let (got, n, off) = app
        .index
        .run(|db| {
            let a = int_at(
                &db.query("SELECT COUNT(*) FROM songs WHERE yt_url != ''", &[])?
                    .rows[0],
                0,
            );
            let n = int_at(&db.query("SELECT COUNT(*) FROM songs", &[])?.rows[0], 0);
            let o = int_at(
                &db.query(
                    "SELECT COUNT(*) FROM songs WHERE yt_url != '' AND yt_official = 1",
                    &[],
                )?
                .rows[0],
                0,
            );
            Ok((a, n, o))
        })
        .await
        .map_err(|e| e.to_string())?;
    log.line(format!(
        "done: {got}/{n} songs have a link ({off} from official/artist channels)"
    ));
    Ok(())
}

fn csv_cell(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// `meta/youtube_misses.csv`: every song without a link, with the best rejected candidate (diagnostics, from the index).
async fn write_misses(app: &App, log: &Log) -> Result<(), String> {
    let rows = app
        .index
        .run(|db| {
            let rows = db.query("SELECT id, title, duration, yt_status, yt_candidate FROM songs WHERE yt_url = '' ORDER BY id", &[])?.rows;
            let mut out = Vec::new();
            for r in rows {
                let who = crate::index::songs::artists_of(db, int_at(&r, 0))?.join(", ");
                out.push((who, r));
            }
            Ok(out)
        })
        .await
        .map_err(|e| e.to_string())?;
    let mut csv = String::from(
        "artists,title,song_id,expected_duration,candidate_title,candidate_channel,candidate_duration,score,candidate_url,status\n",
    );
    for (who, r) in &rows {
        let c = json::parse(&str_at(r, 4)).unwrap_or(Value::Null);
        let field = |k: &str| match c.get(k) {
            Some(Value::String(s)) => s.clone(),
            Some(v) if v.is_number() => v.as_f64().map(|f| f.to_string()).unwrap_or_default(),
            _ => String::new(),
        };
        let status = crate::index::opt_str_at(r, 3).unwrap_or_else(|| "not searched".into());
        let cells = [
            who.clone(),
            str_at(r, 1),
            int_at(r, 0).to_string(),
            opt_int_at(r, 2).map(|d| d.to_string()).unwrap_or_default(),
            field("title"),
            field("channel"),
            field("duration"),
            field("score"),
            field("url"),
            status,
        ];
        csv.push_str(
            &cells
                .iter()
                .map(|c| csv_cell(c))
                .collect::<Vec<_>>()
                .join(","),
        );
        csv.push('\n');
    }
    let path = app.config.meta().join("youtube_misses.csv");
    std::fs::write(&path, csv).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    log.line(format!(
        "Unresolved-song diagnostics: youtube_misses.csv ({} rows)",
        rows.len()
    ));
    Ok(())
}
