use actix_web::dev::{ServiceRequest, ServiceResponse};
use std::sync::Arc;

#[derive(Clone)]
pub struct FilteredLogger {
    skip_prefixes: Arc<Vec<String>>,
}

impl Default for FilteredLogger {
    fn default() -> Self {
        let skip = envmnt::get_or("LOG_SKIP_PREFIXES", "");
        let skip_prefixes = skip
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();
        Self {
            skip_prefixes: Arc::new(skip_prefixes),
        }
    }
}

impl<S, B> actix_web::dev::Transform<S, ServiceRequest> for FilteredLogger
where
    S: actix_web::dev::Service<
            ServiceRequest,
            Response = ServiceResponse<B>,
            Error = actix_web::Error,
        > + 'static,
    B: actix_web::body::MessageBody + 'static,
{
    type Response = ServiceResponse<B>;
    type Error = actix_web::Error;
    type Transform = FilteredLoggerMiddleware<S>;
    type InitError = ();
    type Future = std::future::Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        std::future::ready(Ok(FilteredLoggerMiddleware {
            service,
            skip_prefixes: self.skip_prefixes.clone(),
        }))
    }
}

pub struct FilteredLoggerMiddleware<S> {
    service: S,
    skip_prefixes: Arc<Vec<String>>,
}

impl<S, B> actix_web::dev::Service<ServiceRequest> for FilteredLoggerMiddleware<S>
where
    S: actix_web::dev::Service<
            ServiceRequest,
            Response = ServiceResponse<B>,
            Error = actix_web::Error,
        > + 'static,
    B: actix_web::body::MessageBody + 'static,
{
    type Response = ServiceResponse<B>;
    type Error = actix_web::Error;
    type Future = std::pin::Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

    fn poll_ready(
        &self,
        ctx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.service.poll_ready(ctx)
    }

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let path = req.path().to_string();
        let method = req.method().to_string();
        let skip_prefixes = self.skip_prefixes.clone();
        let fut = self.service.call(req);

        Box::pin(async move {
            match fut.await {
                Ok(res) => {
                    let status = res.status();
                    let mut should_log = !status.is_success();

                    if !should_log {
                        let is_skipped =
                            skip_prefixes.iter().any(|prefix| path.starts_with(prefix));
                        should_log = !is_skipped;
                    }

                    if should_log {
                        tracing::info!("{} {} -> {}", method, path, status);
                    }
                    Ok(res)
                }
                Err(err) => {
                    tracing::error!("{} {} -> Error: {}", method, path, err);
                    Err(err)
                }
            }
        })
    }
}
