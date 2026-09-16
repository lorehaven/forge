use async_trait::async_trait;
use quench_http::prelude::http::{Method, StatusCode};
use quench_http::prelude::{Endpoint, Middleware, Request, Response};
use quench_starter::http::domain::error;
use std::sync::Arc;
use tokio::sync::Semaphore;

pub struct WarehouseLimits {
    upload_semaphore: Arc<Semaphore>,
}

impl WarehouseLimits {
    pub fn new(max_concurrent_uploads: usize) -> Self {
        let permits = max_concurrent_uploads.max(1);
        Self {
            upload_semaphore: Arc::new(Semaphore::new(permits)),
        }
    }
}

#[async_trait]
impl Middleware for WarehouseLimits {
    async fn handle(&self, req: Request, next: &dyn Endpoint) -> Response {
        if !is_upload_mutation(&req) {
            return next.call(req).await;
        }

        let permit = match self.upload_semaphore.clone().try_acquire_owned() {
            Ok(p) => p,
            Err(_) => {
                return error::response(
                    StatusCode::TOO_MANY_REQUESTS,
                    error::DENIED,
                    "too many concurrent upload requests",
                );
            }
        };

        let response = next.call(req).await;
        drop(permit);
        response
    }
}

pub fn is_upload_mutation(req: &Request) -> bool {
    let is_write = matches!(*req.method(), Method::POST | Method::PATCH | Method::PUT);

    is_write && req.uri().path().contains("/blobs/uploads")
}
