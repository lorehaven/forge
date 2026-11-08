//! `IzzyOnDroid`'s download counts: one JSON object, package name to the downloads of the last year.
//!
//! They are the only popularity figures the FOSS apps have (F-Droid publishes none, and nobody rates them), so they
//! fill in what Aptoide cannot: the apps it does not list.

use std::collections::HashMap;

use quench_client::transport::Client;
use quench_serde::{Value, json};

/// The counts in a response body. A name that is empty, or a count that is not a positive number, is left out.
///
/// # Errors
/// The text is not a JSON object.
pub fn parse(text: &str) -> Result<HashMap<String, i64>, String> {
    let value: Value = json::from_str(text).map_err(|e| format!("izzy stats: {e}"))?;
    let object = value
        .as_object()
        .ok_or_else(|| "izzy stats: not an object".to_string())?;
    Ok(object
        .iter()
        .filter(|(name, _)| !name.is_empty())
        .filter_map(|(name, count)| {
            let count = count.as_i64().filter(|n| *n > 0)?;
            Some((name.clone(), count))
        })
        .collect())
}

/// Fetches and reads the counts.
///
/// # Errors
/// The request failed, was refused, or the answer is not the counts.
pub async fn fetch(client: &Client, url: &str) -> Result<HashMap<String, i64>, String> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("izzy stats: {e}"))?;
    let status = response.status().as_u16();
    if status != 200 {
        return Err(format!("izzy stats: answered {status}"));
    }
    parse(
        &response
            .text()
            .await
            .map_err(|e| format!("izzy stats: {e}"))?,
    )
}
