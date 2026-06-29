use quench_cli::prelude::{Tone, print_status};
use std::time::Duration;
use tokio::time::sleep;

pub async fn wait_for_services(service_name: &str, urls: Vec<&str>) {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .danger_accept_invalid_certs(true)
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());

    for url in urls {
        print_status(
            Tone::Info,
            service_name,
            &format!("waiting for dependency: {}", url),
        );

        let mut ready = false;
        while !ready {
            match client.get(url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    ready = true;
                }
                _ => {
                    sleep(Duration::from_secs(1)).await;
                }
            }
        }

        print_status(
            Tone::Success,
            service_name,
            &format!("dependency ready: {}", url),
        );
    }
}
