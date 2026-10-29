//! Aptoide's v7 webservices. The schema is the one confirmed against live requests, not guessed:
//!
//! - `apps/get` is the browse call, paginated (offset/limit). Of the documented `sort` values only `downloads` is
//!   both working and meaningful (`rating` and `trending` answer with nothing, `alpha`/`latest` surface whatever sorts
//!   first), and it stays meaningful far past the first page: still 10M+ download, recognisable apps 100 pages (3000
//!   rows) in. Its `file` object has `vername`/`vercode`/`md5sum`/`filesize` but never `path` or `malware`.
//! - `app/getMeta` is the call for one package; it has everything, including `media` (description, summary), which
//!   lists never carry.
//!
//! There is no "everything" call, so a bounded number of pages is pulled. Because `apps/get` gives no download path,
//! each surviving package is resolved with one `app/getMeta` call, but only after `merge::dedupe_within_source` picked
//! one listing per package (five uploaders of one app cost one call, not five), and with bounded concurrency.

use std::sync::Arc;

use quench_client::transport::Client;
use quench_serde::{Value, json};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use crate::merge::dedupe_within_source;
use crate::models::RawListing;

const PAGE_SIZE: i64 = 30;
const RESOLVE_CONCURRENCY: usize = 20;

/// One app of a response, as a listing. `resolved` says the row came from `app/getMeta` (and so has `media`).
#[must_use]
pub fn parse_app(raw: &Value, resolved: bool) -> Option<RawListing> {
    let package_name = raw["package"].as_str()?.to_string();
    let file = &raw["file"];
    let version_code = file["vercode"].as_i64()?;
    let stats = &raw["stats"];
    let rating = &stats["rating"];
    // The store the app was uploaded to is deliberately not carried: this service is the publisher of record.
    let media = if resolved {
        &raw["media"]
    } else {
        &Value::Null
    };
    Some(RawListing {
        source: "aptoide".to_string(),
        package_name: package_name.clone(),
        label: raw["name"].as_str().unwrap_or(&package_name).to_string(),
        version_code,
        version_name: file["vername"].as_str().unwrap_or("").to_string(),
        size_bytes: file["filesize"]
            .as_i64()
            .or_else(|| raw["size"].as_i64())
            .unwrap_or(0),
        summary: media["summary"].as_str().map(str::to_string),
        description: media["description"].as_str().map(str::to_string),
        icon_url: raw["icon"].as_str().map(str::to_string),
        download_url: file["path"].as_str().map(str::to_string),
        md5: file["md5sum"].as_str().map(str::to_string),
        rating_avg: rating["avg"].as_f64(),
        rating_count: rating["total"].as_i64(),
        downloads: stats["downloads"].as_i64(),
        malware_rank: file["malware"]["rank"].as_str().map(str::to_string),
        ..Default::default()
    })
}

async fn browse_all(
    client: &Client,
    base_url: &str,
    pages: i64,
) -> Result<Vec<RawListing>, String> {
    let mut listings = Vec::new();
    for page in 0..pages {
        let url = format!(
            "{base_url}apps/get?sort=downloads&order=DESC&offset={}&limit={PAGE_SIZE}&trusted=true",
            page * PAGE_SIZE
        );
        let response = client
            .get(url)
            .send()
            .await
            .map_err(|e| format!("aptoide: {e}"))?;
        let status = response.status().as_u16();
        if status != 200 {
            return Err(format!("aptoide: browsing answered {status}"));
        }
        let body: Value = response.json().await.map_err(|e| format!("aptoide: {e}"))?;
        let rows = body["datalist"]["list"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if rows.is_empty() {
            break;
        }
        listings.extend(rows.iter().filter_map(|row| parse_app(row, false)));
    }
    Ok(listings)
}

async fn resolve_one(client: &Client, base_url: &str, package_name: &str) -> Option<RawListing> {
    let url = format!("{base_url}app/getMeta?package_name={package_name}");
    let response = client.get(url).send().await.ok()?;
    if response.status().as_u16() != 200 {
        return None;
    }
    let body: Value = response.json().await.ok()?;
    let full = parse_app(body.get("data")?, true)?;
    full.download_url.is_some().then_some(full)
}

async fn resolve_all(
    client: &Client,
    base_url: &str,
    survivors: Vec<RawListing>,
) -> Vec<RawListing> {
    let gate = Arc::new(Semaphore::new(RESOLVE_CONCURRENCY));
    let mut set = JoinSet::new();
    for listing in survivors {
        let (client, base, gate) = (client.clone(), base_url.to_string(), Arc::clone(&gate));
        set.spawn(async move {
            let _permit = gate.acquire().await.ok()?;
            resolve_one(&client, &base, &listing.package_name).await
        });
    }
    let mut out = Vec::new();
    while let Some(done) = set.join_next().await {
        if let Ok(Some(listing)) = done {
            out.push(listing);
        }
    }
    out
}

/// Browses `pages` pages of the most downloaded apps and resolves one build for each package.
///
/// # Errors
/// Browsing failed (resolving a package that fails just leaves it out).
pub async fn fetch(client: &Client, base_url: &str, pages: i64) -> Result<Vec<RawListing>, String> {
    let base = if base_url.ends_with('/') {
        base_url.to_string()
    } else {
        format!("{base_url}/")
    };
    let raw = browse_all(client, &base, pages).await?;
    let survivors = dedupe_within_source(raw);
    Ok(resolve_all(client, &base, survivors).await)
}

/// A response body as a [`Value`], for the tests.
///
/// # Errors
/// The text is not JSON.
pub fn parse_body(text: &str) -> Result<Value, String> {
    json::from_str(text).map_err(|e| e.to_string())
}
