//! F-Droid's index-v2 format, shared by F-Droid itself and IzzyOnDroid (same repo software, different catalogue).
//! Checked against a live fetch of <https://f-droid.org/repo/index-v2.json>:
//!
//! ```text
//! repo.address                                   -> base URL for relative paths
//! repo.categories[id].name[locale]               -> category id -> localized name
//! packages[pkg].metadata.{name,summary,description,icon,categories,added}[locale]
//! packages[pkg].versions[key].file.{name,sha256,size}
//! packages[pkg].versions[key].manifest.{versionCode,versionName,usesPermission}
//! ```
//!
//! One row per package: the version with the highest `versionCode`.

use std::collections::HashMap;

use quench_client::transport::Client;
use quench_serde::{Value, json};

use crate::models::RawListing;

/// `en-US` first (index-v2's baseline locale, on effectively every package), then `en-GB`, then whatever is first.
fn localized(value: &Value) -> Option<&Value> {
    let obj = value.as_object()?;
    obj.get("en-US")
        .or_else(|| obj.get("en-GB"))
        .or_else(|| obj.values().next())
}

fn localized_str(value: &Value) -> Option<String> {
    localized(value)?.as_str().map(str::to_string)
}

fn category_names(raw: &Value) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let Some(obj) = raw.as_object() else {
        return out;
    };
    for (id, definition) in obj.iter() {
        let name = definition.get("name").and_then(localized_str);
        out.insert(id.clone(), name.unwrap_or_else(|| id.clone()));
    }
    out
}

fn best_version(versions: &Value) -> Option<&Value> {
    let obj = versions.as_object()?;
    obj.values()
        .filter(|entry| entry["manifest"]["versionCode"].as_i64().is_some())
        .max_by_key(|entry| entry["manifest"]["versionCode"].as_i64().unwrap_or(0))
}

/// The listings in an index, and its categories (id to name). Not bound to a source: the caller names it.
///
/// # Errors
/// The text is not JSON.
pub fn parse_index(raw_json: &str) -> Result<(Vec<RawListing>, HashMap<String, String>), String> {
    let data: Value =
        json::from_str(raw_json).map_err(|e| format!("the index is not JSON: {e}"))?;
    let repo = &data["repo"];
    let address = repo["address"].as_str().unwrap_or("").to_string();
    let categories = category_names(&repo["categories"]);

    let mut listings = Vec::new();
    if let Some(packages) = data["packages"].as_object() {
        for (package_name, pkg) in packages.iter() {
            let metadata = &pkg["metadata"];
            let Some(best) = best_version(&pkg["versions"]) else {
                continue;
            };
            let file = &best["file"];
            let (Some(file_name), Some(sha256), Some(size)) = (
                file["name"].as_str(),
                file["sha256"].as_str(),
                file["size"].as_i64(),
            ) else {
                continue;
            };
            let manifest = &best["manifest"];
            let Some(version_code) = manifest["versionCode"].as_i64() else {
                continue;
            };
            let category = metadata["categories"]
                .as_array()
                .and_then(|a| a.iter().find_map(Value::as_str))
                .and_then(|id| categories.get(id))
                .cloned();
            let icon_url = localized(&metadata["icon"])
                .and_then(|entry| entry["name"].as_str())
                .map(|name| format!("{address}{name}"));
            listings.push(RawListing {
                source: String::new(),
                package_name: package_name.clone(),
                label: localized_str(&metadata["name"]).unwrap_or_else(|| package_name.clone()),
                version_code,
                version_name: manifest["versionName"].as_str().unwrap_or("").to_string(),
                size_bytes: size,
                summary: localized_str(&metadata["summary"]),
                description: localized_str(&metadata["description"]),
                icon_url,
                category,
                download_url: Some(format!("{address}{file_name}")),
                sha256: Some(sha256.to_string()),
                ..Default::default()
            });
        }
    }
    Ok((listings, categories))
}

/// Fetches and reads one repository's index, naming every listing after `name`.
///
/// # Errors
/// The index could not be fetched or read.
pub async fn fetch(client: &Client, name: &str, repo_url: &str) -> Result<Vec<RawListing>, String> {
    let response = client
        .get(format!("{}/index-v2.json", repo_url.trim_end_matches('/')))
        .send()
        .await
        .map_err(|e| format!("{name}: {e}"))?;
    let status = response.status().as_u16();
    if status != 200 {
        return Err(format!("{name}: the index answered {status}"));
    }
    let text = response.text().await.map_err(|e| format!("{name}: {e}"))?;
    // The index is tens of megabytes: read it on a blocking thread so the runtime keeps serving.
    let (listings, _) = tokio::task::spawn_blocking(move || parse_index(&text))
        .await
        .map_err(|e| format!("{name}: {e}"))??;
    Ok(listings
        .into_iter()
        .map(|l| RawListing {
            source: name.to_string(),
            ..l
        })
        .collect())
}
