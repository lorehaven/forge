//! Moves a library from SQLite (Songbook 0.1: `meta/library.db` and `meta/user.db`) into Postgres.
//!
//! `songbook-service migrate-sqlite` runs it. The tables must exist already (Foundry installs the `songbook` module). Every row
//! is copied with the id it had, so the playlists, the apps and the links between the tables keep working; the identity
//! counters are moved past the largest id. All of it is one transaction: either every table is copied and its row count checked
//! against the file's, or nothing changes. The SQLite files are only read.
//!
//! A target that already holds rows is refused unless `--replace` empties it first. `--dry-run` does the whole copy and the
//! checks, then rolls back, so the report shows what a real run would do.

use crate::index::{Db, Value};
use quench_pg::Pool;
use std::path::{Path, PathBuf};

/// One table to move: its columns in the order they are copied, and the column (if any) that is an identity in Postgres.
struct Table {
    name: &'static str,
    columns: &'static [&'static str],
    /// The identity column: its counter is moved past the largest id after the copy.
    identity: Option<&'static str>,
    /// Which SQLite file holds it.
    file: File,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum File {
    Library,
    User,
}

const TABLES: &[Table] = &[
    Table {
        name: "artists",
        columns: &["id", "deezer_id", "name", "listed_as"],
        identity: Some("id"),
        file: File::Library,
    },
    Table {
        name: "albums",
        columns: &[
            "id",
            "deezer_id",
            "title",
            "year",
            "type",
            "genre",
            "owner_id",
        ],
        identity: Some("id"),
        file: File::Library,
    },
    Table {
        name: "songs",
        columns: &[
            "id",
            "key",
            "title",
            "filename",
            "duration",
            "yt_url",
            "yt_channel",
            "yt_title",
            "yt_official",
            "yt_score",
            "yt_manual",
            "yt_status",
            "yt_candidate",
            "rank",
            "yt_views",
        ],
        identity: Some("id"),
        file: File::Library,
    },
    Table {
        name: "song_artist",
        columns: &["song_id", "artist_id", "position"],
        identity: None,
        file: File::Library,
    },
    Table {
        name: "song_album",
        columns: &["song_id", "album_id", "track_no"],
        identity: None,
        file: File::Library,
    },
    Table {
        name: "song_track",
        columns: &["deezer_id", "song_id"],
        identity: None,
        file: File::Library,
    },
    Table {
        name: "tags",
        columns: &["id", "kind", "name"],
        identity: Some("id"),
        file: File::Library,
    },
    Table {
        name: "song_tag",
        columns: &["song_id", "tag_id"],
        identity: None,
        file: File::Library,
    },
    Table {
        name: "follows",
        columns: &[
            "id",
            "name",
            "pin",
            "category",
            "position",
            "deezer_name",
            "deezer_id",
            "fans",
            "note",
            "genres",
        ],
        identity: Some("id"),
        file: File::Library,
    },
    Table {
        name: "categories",
        columns: &["id", "name", "position"],
        identity: Some("id"),
        file: File::Library,
    },
    Table {
        name: "blacklist",
        columns: &["id", "artist", "album_key", "title"],
        identity: Some("id"),
        file: File::Library,
    },
    Table {
        name: "suggestions",
        columns: &[
            "id",
            "artist",
            "deezer_id",
            "fans",
            "related_to_count",
            "related_to",
            "scope",
            "position",
        ],
        identity: Some("id"),
        file: File::Library,
    },
    Table {
        name: "suggestion_tracks",
        columns: &["deezer_id", "position", "title", "album", "score"],
        identity: None,
        file: File::Library,
    },
    Table {
        name: "meta",
        columns: &["key", "value"],
        identity: None,
        file: File::Library,
    },
    Table {
        name: "playlists",
        columns: &["id", "owner", "name", "created"],
        identity: Some("id"),
        file: File::User,
    },
    Table {
        name: "playlist_songs",
        columns: &["playlist_id", "position", "song_id"],
        identity: None,
        file: File::User,
    },
    // The plays had no id in SQLite; they are copied in the order they were written and get ids in that order.
    Table {
        name: "plays",
        columns: &["owner", "song_id", "at"],
        identity: Some("id"),
        file: File::User,
    },
    Table {
        name: "contexts",
        columns: &[
            "owner", "key", "kind", "name", "sub", "art_kind", "art_key", "query", "shuffle", "at",
        ],
        identity: None,
        file: File::User,
    },
    Table {
        name: "resume",
        columns: &[
            "owner",
            "song_id",
            "position_ms",
            "queue",
            "idx",
            "device",
            "at",
        ],
        identity: None,
        file: File::User,
    },
];

/// What to move and how.
#[derive(Debug, Clone)]
pub struct Options {
    /// `meta/library.db`.
    pub library: PathBuf,
    /// `meta/user.db`; the playlists are skipped when the file is missing.
    pub user: Option<PathBuf>,
    /// Empty the target tables first instead of refusing a target that holds rows.
    pub replace: bool,
    /// Copy and check, then roll back.
    pub dry_run: bool,
}

/// How one table went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableReport {
    pub name: &'static str,
    /// Rows in the SQLite file; `None` when the file has no such table.
    pub source: Option<u64>,
    /// Rows in Postgres after the copy.
    pub target: u64,
}

/// The rows a SQLite table holds, as parameters; only the columns the file has (older files lack some).
struct Source {
    columns: Vec<&'static str>,
    rows: Vec<Vec<Value>>,
}

fn convert(cell: quench_sqlite::Value, nul: &mut u64) -> Result<Value, String> {
    use quench_sqlite::Value as S;
    Ok(match cell {
        S::Null => Value::Null,
        S::Integer(i) => Value::Integer(i),
        S::Real(f) => Value::Real(f),
        S::Text(t) if t.contains('\0') => {
            // Postgres text cannot hold a NUL.
            *nul += 1;
            Value::Text(t.replace('\0', ""))
        }
        S::Text(t) => Value::Text(t),
        S::Blob(_) => return Err("a blob where text or a number is expected".into()),
    })
}

fn read(
    conn: &mut quench_sqlite::Connection,
    table: &Table,
    nul: &mut u64,
) -> Result<Option<Source>, String> {
    if conn
        .query(&format!("SELECT * FROM {} LIMIT 1", table.name), &[])
        .is_err()
    {
        return Ok(None);
    }
    // Columns added after the first release are missing from an old file: the target's default fills them.
    let columns: Vec<&'static str> = table
        .columns
        .iter()
        .copied()
        .filter(|c| {
            conn.query(&format!("SELECT {c} FROM {} LIMIT 1", table.name), &[])
                .is_ok()
        })
        .collect();
    if columns.is_empty() {
        return Ok(None);
    }
    let order = if table.name == "plays" {
        " ORDER BY rowid"
    } else {
        ""
    };
    let found = conn
        .query(
            &format!("SELECT {} FROM {}{order}", columns.join(", "), table.name),
            &[],
        )
        .map_err(|e| format!("{}: {e}", table.name))?;
    let rows = found
        .rows
        .into_iter()
        .map(|row| {
            row.into_iter()
                .map(|cell| convert(cell, nul))
                .collect::<Result<Vec<_>, _>>()
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("{}: {e}", table.name))?;
    Ok(Some(Source { columns, rows }))
}

fn open(path: &Path) -> Result<quench_sqlite::Connection, String> {
    if !path.is_file() {
        return Err(format!("{} does not exist", path.display()));
    }
    quench_sqlite::Connection::open(path).map_err(|e| format!("{}: {e}", path.display()))
}

async fn count(db: &mut Db, table: &str) -> Result<u64, String> {
    let rows = db
        .query(&format!("SELECT COUNT(*) FROM {table}"), &[])
        .await
        .map_err(|e| format!("{table}: {e}"))?;
    Ok(rows
        .rows
        .first()
        .and_then(|r| r.first())
        .and_then(Value::as_integer)
        .and_then(|n| u64::try_from(n).ok())
        .unwrap_or(0))
}

/// Inserts `rows` in batches that stay under Postgres's limit of parameters per statement.
async fn insert(
    db: &mut Db,
    table: &str,
    columns: &[&str],
    rows: &[Vec<Value>],
) -> Result<(), String> {
    let width = columns.len();
    let per_statement = (60_000 / width).clamp(1, 500);
    for chunk in rows.chunks(per_statement) {
        let marks = vec![format!("({})", vec!["?"; width].join(",")); chunk.len()].join(",");
        let sql = format!(
            "INSERT INTO {table} ({}) VALUES {marks}",
            columns
                .iter()
                .map(|c| format!("\"{c}\""))
                .collect::<Vec<_>>()
                .join(",")
        );
        let params: Vec<Value> = chunk.iter().flatten().cloned().collect();
        db.execute(&sql, &params)
            .await
            .map_err(|e| format!("{table}: {e}"))?;
    }
    Ok(())
}

/// Copies the SQLite files into Postgres; `log` gets one line per step. Returns the per-table report.
pub async fn run(
    pool: &Pool,
    options: &Options,
    log: &(dyn Fn(String) + Sync),
) -> Result<Vec<TableReport>, String> {
    let mut library = open(&options.library)?;
    let mut user = match &options.user {
        Some(path) if path.is_file() => Some(open(path)?),
        Some(path) => {
            log(format!(
                "{} does not exist: the playlists, plays and resume points are not moved",
                path.display()
            ));
            None
        }
        None => None,
    };
    let mut nul = 0;
    let mut sources: Vec<(&Table, Option<Source>)> = Vec::new();
    for table in TABLES {
        let conn = match table.file {
            File::Library => Some(&mut library),
            File::User => user.as_mut(),
        };
        let source = match conn {
            Some(conn) => read(conn, table, &mut nul)?,
            None => None,
        };
        sources.push((table, source));
    }
    if nul > 0 {
        log(format!(
            "{nul} text values held a NUL character; it was removed"
        ));
    }

    let mut db = Db::checkout(pool).await.map_err(|e| e.to_string())?;
    // The tables must be there: Foundry makes them.
    for table in TABLES {
        count(&mut db, table.name)
            .await
            .map_err(|e| format!("{e} (is the songbook module installed? run Foundry first)"))?;
    }
    db.begin().await.map_err(|e| e.to_string())?;
    let report = copy(&mut db, &sources, options, log).await;
    match report {
        Ok(report) if !options.dry_run => {
            db.commit().await.map_err(|e| e.to_string())?;
            log("committed".into());
            Ok(report)
        }
        Ok(report) => {
            db.rollback().await;
            log("dry run: rolled back, nothing was changed".into());
            Ok(report)
        }
        Err(e) => {
            db.rollback().await;
            Err(e)
        }
    }
}

async fn copy(
    db: &mut Db,
    sources: &[(&Table, Option<Source>)],
    options: &Options,
    log: &(dyn Fn(String) + Sync),
) -> Result<Vec<TableReport>, String> {
    if options.replace {
        let all: Vec<&str> = TABLES.iter().map(|t| t.name).collect();
        db.execute(
            &format!("TRUNCATE {} RESTART IDENTITY", all.join(", ")),
            &[],
        )
        .await
        .map_err(|e| e.to_string())?;
        log("emptied the target tables".into());
    } else {
        for table in TABLES {
            let rows = count(db, table.name).await?;
            if rows > 0 {
                return Err(format!(
                    "{} already holds {rows} rows; nothing was copied. Use --replace to empty the tables first",
                    table.name
                ));
            }
        }
    }
    let mut report = Vec::new();
    for (table, source) in sources {
        let Some(source) = source else {
            log(format!("{}: not in the file, skipped", table.name));
            report.push(TableReport {
                name: table.name,
                source: None,
                target: 0,
            });
            continue;
        };
        insert(db, table.name, &source.columns, &source.rows).await?;
        if let Some(id) = table.identity
            && source.columns.contains(&id)
        {
            // The next row the service writes gets an id past every copied one.
            db.query(
                &format!(
                    "SELECT setval(pg_get_serial_sequence('{t}', '{id}'), GREATEST((SELECT COALESCE(MAX({id}), 0) FROM {t}), 1), (SELECT MAX({id}) IS NOT NULL FROM {t}))",
                    t = table.name
                ),
                &[],
            )
            .await
            .map_err(|e| format!("{}: {e}", table.name))?;
        }
        let target = count(db, table.name).await?;
        let expected = source.rows.len() as u64;
        if target != expected {
            return Err(format!(
                "{}: {expected} rows in the file but {target} in Postgres; nothing was changed",
                table.name
            ));
        }
        log(format!("{}: {target} rows", table.name));
        report.push(TableReport {
            name: table.name,
            source: Some(expected),
            target,
        });
    }
    Ok(report)
}

/// The usage text of `songbook-service migrate-sqlite`.
pub const USAGE: &str = "\
Moves a Songbook 0.1 library (SQLite) into Postgres.

Usage: songbook-service migrate-sqlite [OPTIONS]

  --home DIR            the music home; reads DIR/meta/library.db and DIR/meta/user.db (default: $MUSIC_HOME, else .)
  --library FILE        the library index, when it is not DIR/meta/library.db
  --user FILE           the playlists file, when it is not DIR/meta/user.db
  --database-url URL    the Postgres to write to (default: $DATABASE_URL)
  --schema NAME         the schema Foundry installed the songbook module in (default: $DB_SCHEMA, else songbook)
  --replace             empty the songbook tables first (without it a target that holds rows is refused)
  --dry-run             copy and check everything, then roll back
  -h, --help            this text

Stop the 0.1 service first: it holds the SQLite files. The files are only read. The tables must exist (Foundry:
`foundry apply --install songbook`). All of it is one transaction.
";

/// Parses the arguments after `migrate-sqlite` into the options, the database URL and the schema.
pub fn parse_args(args: &[String]) -> Result<(Options, String, String), String> {
    let mut home = quench_core::env::get_or("MUSIC_HOME", ".");
    let (mut library, mut user) = (None, None);
    let mut url = quench_core::env::get_any(&["DATABASE_URL", "POSTGRES_URL"], "");
    let mut schema = quench_core::env::get_or("DB_SCHEMA", "songbook");
    let (mut replace, mut dry_run) = (false, false);
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let mut value = |name: &str| {
            it.next()
                .cloned()
                .ok_or_else(|| format!("{name} needs a value"))
        };
        match arg.as_str() {
            "--home" => home = value("--home")?,
            "--library" => library = Some(PathBuf::from(value("--library")?)),
            "--user" => user = Some(PathBuf::from(value("--user")?)),
            "--database-url" => url = value("--database-url")?,
            "--schema" => schema = value("--schema")?,
            "--replace" => replace = true,
            "--dry-run" => dry_run = true,
            other => return Err(format!("unknown option {other}")),
        }
    }
    if url.trim().is_empty() {
        return Err("no database: pass --database-url or set DATABASE_URL".into());
    }
    let meta = PathBuf::from(&home).join("meta");
    Ok((
        Options {
            library: library.unwrap_or_else(|| meta.join("library.db")),
            user: Some(user.unwrap_or_else(|| meta.join("user.db"))),
            replace,
            dry_run,
        },
        url,
        schema,
    ))
}

/// The command: parses `args`, connects, copies, prints the report. `Err` is the message to print.
pub async fn command(args: &[String]) -> Result<(), String> {
    if args.iter().any(|a| a == "-h" || a == "--help") {
        println!("{USAGE}");
        return Ok(());
    }
    let (options, url, schema) = parse_args(args)?;
    let pool = crate::index::db::connect(&url, &schema)
        .await
        .map_err(|e| format!("database: {e}"))?;
    println!(
        "library {}\nplaylists {}\ntarget schema {schema}{}",
        options.library.display(),
        options
            .user
            .as_ref()
            .map_or_else(String::new, |p| p.display().to_string()),
        if options.dry_run { " (dry run)" } else { "" }
    );
    run(&pool, &options, &|line| println!("{line}")).await?;
    Ok(())
}
