use actix_web::Error;
use actix_web::HttpMessage;
use actix_web::dev::{Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::http::header::{HeaderName, HeaderValue};
use futures_util::future::LocalBoxFuture;
use std::rc::Rc;
use uuid::Uuid;

const CORRELATION_ID_HEADER: &str = "X-Correlation-ID";

/// Middleware that adds or propagates correlation IDs for request tracing
pub struct CorrelationIdMiddleware;

impl<S, B> Transform<S, ServiceRequest> for CorrelationIdMiddleware
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type InitError = ();
    type Transform = CorrelationIdMiddlewareService<S>;
    type Future = std::future::Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        std::future::ready(Ok(CorrelationIdMiddlewareService {
            service: Rc::new(service),
        }))
    }
}

pub struct CorrelationIdMiddlewareService<S> {
    service: Rc<S>,
}

impl<S, B> Service<ServiceRequest> for CorrelationIdMiddlewareService<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(
        &self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    #[allow(unused_mut)]
    fn call(&self, mut req: ServiceRequest) -> Self::Future {
        let correlation_id = req
            .headers()
            .get(CORRELATION_ID_HEADER)
            .and_then(|h| h.to_str().ok())
            .map(|s| s.to_string())
            .unwrap_or_else(|| Uuid::new_v4().to_string());

        // Store in request extensions for access in handlers
        req.extensions_mut().insert(correlation_id.clone());

        // Store in thread-local context for tracing
        tracing::Span::current().record("correlation_id", &correlation_id);

        let service = self.service.clone();
        Box::pin(async move {
            let res = service.call(req).await?;
            let mut response = res;

            // Add correlation ID to response headers
            if let Ok(header_value) = HeaderValue::from_str(&correlation_id) {
                response
                    .headers_mut()
                    .insert(HeaderName::from_static(CORRELATION_ID_HEADER), header_value);
            }

            Ok(response)
        })
    }
}

/// Get the current correlation ID from request extensions
pub fn get_correlation_id(extensions: &actix_web::dev::Extensions) -> String {
    extensions
        .get::<String>()
        .cloned()
        .unwrap_or_else(|| "unknown".to_string())
}

/// Helper to inject correlation ID into outgoing HTTP requests
pub fn inject_correlation_id_header(
    builder: reqwest::RequestBuilder,
    correlation_id: &str,
) -> reqwest::RequestBuilder {
    builder.header(CORRELATION_ID_HEADER, correlation_id)
}
