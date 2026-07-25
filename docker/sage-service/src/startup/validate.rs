use crate::clients::switchboard::SwitchboardClient;
use crate::config::SageConfig;
use anyhow::Result;

/// Validate critical service dependencies at startup
pub async fn validate_startup(switchboard: &SwitchboardClient, config: &SageConfig) -> Result<()> {
    tracing::info!("Validating startup configuration...");

    // Check if validation is disabled (for testing)
    let skip_switchboard_check = std::env::var("SKIP_SWITCHBOARD_CHECK")
        .map(|v| v.to_lowercase() == "true")
        .unwrap_or(false);

    // Validate Switchboard connectivity (unless disabled)
    if !skip_switchboard_check {
        validate_switchboard(switchboard).await?;
    } else {
        tracing::warn!("Switchboard connectivity validation skipped (SKIP_SWITCHBOARD_CHECK=true)");
    }

    // Validate search providers (if configured)
    if !config.available_search_providers.is_empty() {
        validate_search_providers(config).await?;
    }

    tracing::info!("All startup validations passed");
    Ok(())
}

/// Validate Switchboard service is reachable and healthy
async fn validate_switchboard(switchboard: &SwitchboardClient) -> Result<()> {
    tracing::info!("Checking Switchboard connectivity...");

    match switchboard.get_vllm_instances().await {
        Ok(instances) => {
            tracing::info!(
                "Switchboard is reachable. {} vLLM instances currently running",
                instances.len()
            );
            Ok(())
        }
        Err(e) => {
            let error_msg = format!(
                "Failed to connect to Switchboard service: {}. \
                 Make sure SWITCHBOARD_URL is correct and the service is running.",
                e
            );
            tracing::error!("{}", error_msg);
            Err(anyhow::anyhow!(error_msg))
        }
    }
}

/// Validate configured search providers are accessible
async fn validate_search_providers(config: &SageConfig) -> Result<()> {
    tracing::info!(
        "Validating search providers: {}",
        config.available_search_providers.join(", ")
    );

    // This is a placeholder for more detailed validation
    // Different providers may need different checks
    for provider in &config.available_search_providers {
        match provider.as_str() {
            "brave" => {
                if std::env::var("BRAVE_API_KEY").is_err() {
                    tracing::warn!("Brave Search provider configured but BRAVE_API_KEY is not set");
                } else {
                    tracing::debug!("Brave Search provider is configured");
                }
            }
            "searxng" => {
                let url = envmnt::get_or("SEARXNG_INSTANCE_URL", "https://searxng.be");
                tracing::debug!("SearXNG provider configured with URL: {}", url);
            }
            "serpapi" => {
                if std::env::var("SERPAPI_API_KEY").is_err() {
                    tracing::warn!("SerpAPI provider configured but SERPAPI_API_KEY is not set");
                } else {
                    tracing::debug!("SerpAPI provider is configured");
                }
            }
            "duckduckgo" => {
                tracing::debug!("DuckDuckGo provider is available (no API key required)");
            }
            _ => {
                tracing::warn!("Unknown search provider: {}", provider);
            }
        }
    }

    Ok(())
}
