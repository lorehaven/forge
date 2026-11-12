//! similar: suggest artists related to the ones you follow, ranked by how many of yours point at them.

use crate::app::App;
use crate::index::follow;
use crate::index::songs::UNCATEGORIZED;
use crate::jobs::Log;
use songbook_core::naming::norm;
use songbook_core::{Suggestion, TopTrack, popularity};
use std::collections::{HashMap, HashSet};

/// Ranks candidates: `(artist id, followed artists that point at it, its Deezer name, fans)`; more pointers first, then
/// more fans.
#[must_use]
pub fn rank(
    mut candidates: Vec<(i64, Vec<String>, String, i64)>,
    top: usize,
) -> Vec<(i64, Vec<String>, String, i64)> {
    candidates.sort_by(|a, b| {
        b.1.len()
            .cmp(&a.1.len())
            .then_with(|| b.3.cmp(&a.3))
            .then_with(|| a.0.cmp(&b.0))
    });
    candidates.truncate(top);
    candidates
}

/// How many of a suggested artist's best songs are kept.
pub const BEST: u32 = 5;

pub async fn run(
    app: &App,
    top: u32,
    per_artist: u32,
    scope: &str,
    log: &Log,
) -> Result<(), String> {
    let (follows, categories, cmap) = app
        .index
        .run(async |db| {
            Ok((
                follow::follows(db).await?,
                follow::categories(db).await?,
                follow::category_map(db).await?,
            ))
        })
        .await
        .map_err(|e| e.to_string())?;
    let scope = scope.trim();
    if !scope.is_empty()
        && !categories
            .iter()
            .chain([&UNCATEGORIZED.to_string()])
            .any(|c| norm(c) == norm(scope))
    {
        return Err(format!("unknown category: {scope}"));
    }
    // mine: every followed artist (never suggested); seeds: those whose related artists we ask about.
    let mut mine: HashMap<i64, String> = HashMap::new();
    let mut seeds: Vec<(i64, String)> = Vec::new();
    for f in &follows {
        if let Ok(id) = f.deezer_id.parse::<i64>() {
            mine.insert(id, f.deezer_name.clone());
            let category = cmap.get(&f.name).map_or(UNCATEGORIZED, String::as_str);
            if scope.is_empty() || norm(category) == norm(scope) {
                seeds.push((id, f.deezer_name.clone()));
            }
        }
    }
    if mine.is_empty() {
        return Err("no synced artists: run sync first".into());
    }
    if seeds.is_empty() {
        return Err(format!(
            "no synced artists in scope: {}",
            if scope.is_empty() { "all" } else { scope }
        ));
    }
    log.line(format!(
        "scope: {} ({} of {} artists)",
        if scope.is_empty() {
            "all categories"
        } else {
            scope
        },
        seeds.len(),
        mine.len()
    ));
    let names: HashSet<String> = mine.values().map(|n| norm(n)).collect();
    let mut found: HashMap<i64, (Vec<String>, String, i64)> = HashMap::new();
    for (id, name) in &seeds {
        for rel in app.deezer.related(*id, per_artist).await? {
            if mine.contains_key(&rel.id) || names.contains(&norm(&rel.name)) {
                continue;
            }
            let entry = found
                .entry(rel.id)
                .or_insert_with(|| (Vec::new(), rel.name.clone(), rel.fans));
            entry.0.push(name.clone());
        }
    }
    let ranked = rank(
        found
            .into_iter()
            .map(|(id, (via, name, fans))| (id, via, name, fans))
            .collect(),
        top as usize,
    );
    let mut list: Vec<Suggestion> = Vec::new();
    for (id, via, name, fans) in &ranked {
        // Their best songs, so the list says what to expect. One failure leaves that artist without songs, not the run.
        let best = match app.deezer.top_tracks(*id, BEST).await {
            Ok(tracks) => tracks
                .into_iter()
                .map(|t| TopTrack {
                    title: t.title,
                    album: t.album,
                    score: popularity(Some(t.rank), None).unwrap_or(0),
                })
                .collect(),
            Err(e) => {
                log.line(format!("  no best songs for {name}: {e}"));
                Vec::new()
            }
        };
        list.push(Suggestion {
            artist: name.clone(),
            deezer_id: id.to_string(),
            fans: fans.to_string(),
            related_to_count: via.len() as u32,
            related_to: via.join("; "),
            scope: scope.to_string(),
            best,
        });
    }
    for s in list.iter().take(20) {
        let via: Vec<&str> = s.related_to.split("; ").take(4).collect();
        log.line(format!(
            "{:<28} {:>2}  via {}",
            s.artist,
            s.related_to_count,
            via.join(", ")
        ));
    }
    let n = list.len();
    app.index
        .run(async move |db| follow::set_suggestions(db, &list).await)
        .await
        .map_err(|e| e.to_string())?;
    log.line(format!("{n} suggestions saved"));
    Ok(())
}
