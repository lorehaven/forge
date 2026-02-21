use crate::shared::docker_error;
use actix_web::{
    Error,
    body::{EitherBody, MessageBody},
    dev::{Service, ServiceRequest, ServiceResponse, Transform},
};
use futures_util::future::{LocalBoxFuture, Ready, ok};
use std::sync::Arc;
use std::task::{Context, Poll};
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

impl<S, B> Transform<S, ServiceRequest> for WarehouseLimits
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    B: MessageBody + 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type Transform = WarehouseLimitsMiddleware<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ok(WarehouseLimitsMiddleware {
            service,
            upload_semaphore: self.upload_semaphore.clone(),
        })
    }
}

pub struct WarehouseLimitsMiddleware<S> {
    service: S,
    upload_semaphore: Arc<Semaphore>,
}

impl<S, B> Service<ServiceRequest> for WarehouseLimitsMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    B: MessageBody + 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&self, ctx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(ctx)
    }

    fn call(&self, req: ServiceRequest) -> Self::Future {
        if !is_upload_mutation(&req) {
            let fut = self.service.call(req);
            return Box::pin(async move {
                let res = fut.await?;
                Ok(res.map_into_left_body())
            });
        }

        let permit = match self.upload_semaphore.clone().try_acquire_owned() {
            Ok(p) => p,
            Err(_) => {
                let response = docker_error::response(
                    actix_web::http::StatusCode::TOO_MANY_REQUESTS,
                    docker_error::DENIED,
                    "too many concurrent upload requests",
                )
                .map_into_right_body();
                return Box::pin(async move { Ok(req.into_response(response)) });
            }
        };

        let fut = self.service.call(req);
        Box::pin(async move {
            let _permit = permit;
            let res = fut.await?;
            Ok(res.map_into_left_body())
        })
    }
}

fn is_upload_mutation(req: &ServiceRequest) -> bool {
    let is_write = matches!(
        *req.method(),
        actix_web::http::Method::POST
            | actix_web::http::Method::PATCH
            | actix_web::http::Method::PUT
    );

    is_write && req.path().contains("/blobs/uploads")
}
