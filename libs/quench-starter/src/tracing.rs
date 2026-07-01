/// OpenTelemetry tracing configuration for distributed request tracing
use tracing::{subscriber::set_global_default, Level};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

/// Initialize tracing with OpenTelemetry exports
/// Configures both console logging and optional Jaeger export
pub fn init_tracing() {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));

    let subscriber = tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt::layer().with_target(true));

    set_global_default(subscriber)
        .expect("Failed to set tracing subscriber");
}

/// Initialize OpenTelemetry with Jaeger exporter for distributed tracing
/// Requires JAEGER_ENDPOINT environment variable (e.g., http://localhost:14268/api/traces)
#[cfg(feature = "jaeger")]
pub fn init_jaeger_tracing(service_name: &str) {
    use opentelemetry::sdk::trace::{self, Sampler};
    use opentelemetry::sdk::Resource;
    use opentelemetry_jaeger::new_agent_pipeline;
    use tracing_opentelemetry::OpenTelemetryLayer;
    use opentelemetry::KeyValue;

    // Configure Jaeger exporter
    let jaeger_endpoint = std::env::var("JAEGER_ENDPOINT")
        .unwrap_or_else(|_| "http://localhost:14268/api/traces".to_string());

    let tracer = new_agent_pipeline()
        .with_endpoint(&jaeger_endpoint)
        .with_service_name(service_name)
        .with_sample_rate(0.1) // Sample 10% of traces in production
        .install_simple()
        .expect("Failed to install Jaeger tracer");

    let telemetry = OpenTelemetryLayer::new(tracer);
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));

    let subscriber = tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt::layer())
        .with(telemetry);

    set_global_default(subscriber)
        .expect("Failed to set tracing subscriber");
}

/// Configuration for trace sampling
#[derive(Debug, Clone)]
pub struct TracingConfig {
    /// Sample rate: 0.0 to 1.0
    /// 0.0 = no sampling, 1.0 = sample all traces
    pub sample_rate: f64,
    /// Enable Jaeger export
    pub jaeger_enabled: bool,
    /// Jaeger endpoint URL
    pub jaeger_endpoint: String,
    /// Service name for Jaeger
    pub service_name: String,
}

impl Default for TracingConfig {
    fn default() -> Self {
        Self {
            sample_rate: 0.1,
            jaeger_enabled: std::env::var("JAEGER_ENABLED").is_ok(),
            jaeger_endpoint: std::env::var("JAEGER_ENDPOINT")
                .unwrap_or_else(|_| "http://localhost:14268/api/traces".to_string()),
            service_name: std::env::var("SERVICE_NAME").unwrap_or_else(|_| "unknown".to_string()),
        }
    }
}
