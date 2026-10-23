use crate::clients::switchboard::SwitchboardClient;
use crate::config::SageConfig;
use quench_core::Result;

/// Validate critical service dependencies at startup
pub async fn validate_startup(switchboard: &SwitchboardClient, config: &SageConfig) -> Result<()> {
    quench_log::info!("Validating startup configuration...");

    // Check if validation is disabled (for testing)
    let skip_switchboard_check =
        std::env::var("SKIP_SWITCHBOARD_CHECK").is_ok_and(|v| v.to_lowercase() == "true");

    // Validate Switchboard connectivity (unless disabled)
    if skip_switchboard_check {
        quench_log::warn!(
            "Switchboard connectivity validation skipped (SKIP_SWITCHBOARD_CHECK=true)"
        );
    } else {
        validate_switchboard(switchboard).await?;
    }

    // Validate search providers (if configured)
    if !config.available_search_providers.is_empty() {
        validate_search_providers(config);
    }

    quench_log::info!("All startup validations passed");
    Ok(())
}

/// Validate Switchboard service is reachable and healthy
async fn validate_switchboard(switchboard: &SwitchboardClient) -> Result<()> {
    quench_log::info!("Checking Switchboard connectivity...");

    match switchboard.get_vllm_instances().await {
        Ok(instances) => {
            quench_log::info!(
                "Switchboard is reachable. {} vLLM instances currently running",
                instances.len()
            );
            Ok(())
        }
        Err(e) => {
            let error_msg = format!(
                "Failed to connect to Switchboard service: {e}. \
                 Make sure SWITCHBOARD_URL is correct and the service is running."
            );
            quench_log::error!("{}", error_msg);
            Err(quench_core::err!(error_msg))
        }
    }
}

/// Validate configured search providers are accessible
fn validate_search_providers(config: &SageConfig) {
    quench_log::info!(
        "Validating search providers: {}",
        config.available_search_providers.join(", ")
    );

    // Placeholder for more detailed, provider-specific validation.
    for provider in &config.available_search_providers {
        match provider.as_str() {
            "brave" => {
                if std::env::var("BRAVE_API_KEY").is_err() {
                    quench_log::warn!(
                        "Brave Search provider configured but BRAVE_API_KEY is not set"
                    );
                } else {
                    quench_log::debug!("Brave Search provider is configured");
                }
            }
            "searxng" => {
                let url = quench_core::env::get_or("SEARXNG_INSTANCE_URL", "https://searxng.be");
                quench_log::debug!("SearXNG provider configured with URL: {}", url);
            }
            "serpapi" => {
                if std::env::var("SERPAPI_API_KEY").is_err() {
                    quench_log::warn!("SerpAPI provider configured but SERPAPI_API_KEY is not set");
                } else {
                    quench_log::debug!("SerpAPI provider is configured");
                }
            }
            "duckduckgo" => {
                quench_log::debug!("DuckDuckGo provider is available (no API key required)");
            }
            _ => {
                quench_log::warn!("Unknown search provider: {}", provider);
            }
        }
    }
}
