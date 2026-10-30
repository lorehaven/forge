use lectern_service::config::Config;
use lectern_service::library::Library;
use lectern_service::site::Site;
use lectern_service::source::WarehouseSource;
use quench_http::prelude::*;
use quench_starter::http::middleware::{
    correlation::CorrelationId, logger::FilteredLogger, security_headers::SecurityHeaders,
};
use quench_starter::http::serve_app;
use std::sync::Arc;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    quench_starter::logging::init();
    quench_log::info!("Lectern starting");

    let config = Config::from_env().unwrap_or_else(|reason| panic!("configuration: {reason}"));
    let credentials = if let (Some(gatehouse), Some(secret)) =
        (&config.gatehouse_url, &config.client_secret)
    {
        Some((
            gatehouse.as_str(),
            config.client_id.as_str(),
            secret.as_str(),
        ))
    } else {
        quench_log::warn!(
            "no client credentials (GATEHOUSE_URL, WAREHOUSE_CLIENT_SECRET): asking Warehouse without a token, \
             which only one with authentication off will answer"
        );
        None
    };
    let source = WarehouseSource::new(
        &config.warehouse_url,
        &config.storage,
        credentials,
        config.tls_verify,
    );
    let library = Arc::new(Library::new(&config.base_path));

    let refresher = library.clone();
    let interval = config.refresh;
    tokio::spawn(async move {
        loop {
            match refresher.refresh(&source).await {
                Ok(report) => {
                    for loaded in &report.loaded {
                        quench_log::info!("serving {loaded}");
                    }
                    for gone in &report.removed {
                        quench_log::info!("{gone} is gone from the storage");
                    }
                    for failure in &report.failed {
                        quench_log::error!("could not update {failure}");
                    }
                }
                Err(reason) => quench_log::error!("could not list the books: {reason}"),
            }
            tokio::time::sleep(interval).await;
        }
    });

    let site: Arc<dyn Endpoint> = Arc::new(Site::new(library, &config.base_path));
    let app = wrap(site, SecurityHeaders);
    let app = wrap(app, CorrelationId);
    let app = wrap(app, FilteredLogger::default());

    let container = Arc::new(
        ContainerBuilder::new()
            .build()
            .await
            .unwrap_or_else(|e| panic!("dependency graph failed to resolve: {e}")),
    );
    serve_app("lectern-service", app, container).await
}
