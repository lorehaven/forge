//! Agents Gauge can read. Each turns a scrape into the same [`Reading`](super::model::Reading).

use super::config::{Kind, Target};
use super::model::Reading;
use quench_client::transport::Client;

pub mod node;
pub mod pcp;

#[must_use]
pub fn client() -> Client {
    Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .user_agent("gauge")
        .build()
        .unwrap_or_default()
}

pub(crate) async fn get_text(http: &Client, url: &str) -> Result<String, String> {
    let response = http.get(url).send().await.map_err(|e| e.to_string())?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("{url}: {status}"));
    }
    response.text().await.map_err(|e| e.to_string())
}

pub async fn scrape(http: &Client, target: &Target) -> Result<Reading, String> {
    let mut reading = match target.kind {
        Kind::Node => node::scrape(http, &target.url).await?,
        Kind::Pcp => pcp::scrape(http, &target.url).await?,
    };
    // The side agent is a bonus: if it is down the host is still up, just without its GPUs.
    if let Some(also) = &target.also
        && let Ok(extra) = node::scrape(http, also).await
    {
        reading.gpus.extend(extra.gpus);
        reading.temps.extend(extra.temps);
    }
    Ok(reading)
}

pub(crate) fn now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64())
}
