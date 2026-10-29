//! The two-pass dedup/merge this whole service exists for.
//!
//! Pass 1, [`dedupe_within_source`]: some sources (Aptoide) can list the
//! same package more than once — a different uploader's "store" each time.
//! One listing per (source, package) survives, ranked by Aptoide's own
//! malware-scan verdict first, then downloads, then rating count —
//! deterministic, no ties left to chance.
//!
//! Pass 2, [`merge_all`]: one row per package across *all* sources, by
//! per-field precedence rather than winner-take-all, so a package doesn't
//! lose Aptoide's rating data just because F-Droid is the safer install
//! source (and vice versa — an Aptoide-only package still gets a row, an
//! F-Droid one still gets a category).

use std::collections::HashMap;

use crate::models::{MergedPackage, RawListing};

pub const DEFAULT_SOURCE_PRIORITY: &[&str] = &["fdroid", "izzyondroid", "aptoide"];

fn within_source_rank(listing: &RawListing) -> (i32, i64, i64) {
    (
        if listing.malware_rank.as_deref() == Some("TRUSTED") {
            1
        } else {
            0
        },
        listing.downloads.unwrap_or(0),
        listing.rating_count.unwrap_or(0),
    )
}

/// One listing per (source, package_name). Only matters for a source that
/// can list the same package more than once (Aptoide); a source with one
/// listing per package (F-Droid, IzzyOnDroid) passes through unchanged.
pub fn dedupe_within_source(listings: Vec<RawListing>) -> Vec<RawListing> {
    let mut best: HashMap<(String, String), RawListing> = HashMap::new();
    for listing in listings {
        let key = (listing.source.clone(), listing.package_name.clone());
        match best.get(&key) {
            Some(current) if within_source_rank(current) >= within_source_rank(&listing) => {}
            _ => {
                best.insert(key, listing);
            }
        }
    }
    best.into_values().collect()
}

fn pick<'a>(
    per_source: &'a HashMap<String, RawListing>,
    priority: &[&str],
    field: impl Fn(&'a RawListing) -> Option<&'a str>,
) -> Option<String> {
    for source in priority {
        if let Some(listing) = per_source.get(*source)
            && let Some(value) = field(listing)
        {
            return Some(value.to_string());
        }
    }
    None
}

/// `listings_by_source` need not be pre-deduped — this calls
/// [`dedupe_within_source`] itself per source before merging across them.
pub fn merge_all(
    listings_by_source: HashMap<String, Vec<RawListing>>,
    source_priority: &[&str],
) -> Vec<MergedPackage> {
    let mut by_package: HashMap<String, HashMap<String, RawListing>> = HashMap::new();
    for (source, listings) in listings_by_source {
        for listing in dedupe_within_source(listings) {
            by_package
                .entry(listing.package_name.clone())
                .or_default()
                .insert(source.clone(), listing);
        }
    }

    let mut merged = Vec::new();
    for (package_name, per_source) in by_package {
        let install_listing = source_priority
            .iter()
            .find_map(|source| per_source.get(*source).filter(|l| l.download_url.is_some()));

        let Some(install_listing) = install_listing else {
            // Nothing in any source resolved to an actual download for this
            // package (e.g. an Aptoide row that never made it through
            // getMeta resolution) — nothing installable, so no row.
            continue;
        };

        let aptoide = per_source.get("aptoide");

        merged.push(MergedPackage {
            package_name: package_name.clone(),
            label: pick(&per_source, source_priority, |l| Some(l.label.as_str()))
                .unwrap_or(package_name),
            version_code: install_listing.version_code,
            version_name: install_listing.version_name.clone(),
            size_bytes: install_listing.size_bytes,
            download_url: install_listing.download_url.clone().unwrap(),
            install_source: install_listing.source.clone(),
            summary: pick(&per_source, source_priority, |l| l.summary.as_deref()),
            description: pick(&per_source, source_priority, |l| l.description.as_deref()),
            icon_url: pick(&per_source, source_priority, |l| l.icon_url.as_deref()),
            category: pick(&per_source, source_priority, |l| l.category.as_deref()),
            sha256: install_listing.sha256.clone(),
            md5: install_listing.md5.clone(),
            rating_avg: aptoide.and_then(|a| a.rating_avg),
            rating_count: aptoide.and_then(|a| a.rating_count),
            downloads: aptoide.and_then(|a| a.downloads),
            // Not aptoide.malware_rank: it belongs to whichever listing
            // actually won the install slot, so it reads correctly even in
            // the (currently impossible, but not worth relying on) case
            // where Aptoide isn't the install source.
            malware_rank: install_listing.malware_rank.clone(),
        });
    }
    merged
}
