//! One-time import of the Python helper's text files (`artists.txt`, `categories.txt`, `blacklist.txt`,
//! `artist_matches.csv`, `suggestions.csv`) into the tables that replace them. Runs on a start where `follows` is empty and
//! `artists.txt` exists; afterwards the files are never read again.

use super::follow;
use super::{Result, int, text};
use quench_sqlite::Connection;
use songbook_core::naming::norm;
use std::path::Path;

/// Splits one CSV record: commas, with `"` quoting (`""` for a quote inside).
#[must_use]
pub fn csv_fields(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            ('"', _) => quoted = !quoted,
            (',', false) => out.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    out.push(cur);
    out
}

fn read_lines(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .map(|t| t.lines().map(str::to_string).collect())
        .unwrap_or_default()
}

/// Rows of a CSV file with a header, as `(header names, record)`; records are padded to the header's width.
fn csv_rows(path: &Path) -> Vec<Vec<String>> {
    let lines = read_lines(path);
    let Some((header, rest)) = lines.split_first() else {
        return Vec::new();
    };
    let width = csv_fields(header).len();
    rest.iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let mut f = csv_fields(l);
            f.resize(width.max(f.len()), String::new());
            f
        })
        .collect()
}

/// Imports the files in `meta_dir` if `follows` is empty. Returns how many artists were imported.
pub fn once(db: &mut Connection, meta_dir: &Path) -> Result<usize> {
    if !follow::follows(db)?.is_empty() {
        return Ok(0);
    }
    let artists_file = meta_dir.join("artists.txt");
    if !artists_file.is_file() {
        return Ok(0);
    }
    db.transaction(|db| {
        for (i, name) in read_lines(&meta_dir.join("categories.txt"))
            .iter()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .enumerate()
        {
            db.execute(
                "INSERT OR IGNORE INTO categories(name, position) VALUES (?, ?)",
                &[text(name), int(i as i64 + 1)],
            )?;
        }
        let known = follow::categories(db)?;
        let mut count = 0;
        for line in read_lines(&artists_file) {
            let line = line
                .split('#')
                .next()
                .unwrap_or_default()
                .trim()
                .to_string();
            if line.is_empty() {
                continue;
            }
            let mut parts = line.split('|').map(str::trim);
            let name = parts.next().unwrap_or_default();
            let pin: String = parts
                .next()
                .unwrap_or_default()
                .chars()
                .filter(char::is_ascii_digit)
                .collect();
            let category = parts.next().unwrap_or_default();
            let category = known
                .iter()
                .find(|c| norm(c) == norm(category))
                .cloned()
                .unwrap_or_default();
            count += db.execute(
                "INSERT OR IGNORE INTO follows(name, pin, category, position) VALUES (?,?,?,?)",
                &[
                    text(name),
                    text(&pin),
                    text(&category),
                    int(count as i64 + 1),
                ],
            )? as usize;
        }
        for line in read_lines(&meta_dir.join("blacklist.txt")) {
            let line = line.split('#').next().unwrap_or_default().to_string();
            if let Some((artist, album)) = line.split_once('|')
                && !album.trim().is_empty()
            {
                follow::blacklist_add(db, artist.trim(), album.trim())?;
            }
        }
        for r in csv_rows(&meta_dir.join("artist_matches.csv")) {
            if r.len() >= 5 {
                follow::set_match(db, &r[0], &r[1], &r[2], &r[3], &r[4])?;
            }
        }
        let list: Vec<songbook_core::Suggestion> = csv_rows(&meta_dir.join("suggestions.csv"))
            .into_iter()
            .filter(|r| r.len() >= 6)
            .map(|r| songbook_core::Suggestion {
                artist: r[0].clone(),
                deezer_id: r[1].clone(),
                fans: r[2].clone(),
                related_to_count: r[3].parse().unwrap_or(0),
                related_to: r[4].clone(),
                scope: r[5].clone(),
                best: Vec::new(),
            })
            .collect();
        follow::set_suggestions(db, &list)?;
        quench_log::info!(
            "imported {count} followed artists from {}",
            meta_dir.display()
        );
        Ok(count)
    })
}
