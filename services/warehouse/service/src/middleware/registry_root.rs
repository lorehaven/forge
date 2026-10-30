//! Keeps the registry protocol (`/v2`, `/token`) at the server root only.
//! Route discovery also mounts those routes under `BASE_PATH`, where `WarehouseAuth` never runs.

use quench_core::async_trait;
use quench_http::prelude::http::StatusCode;
use quench_http::prelude::{Endpoint, Middleware, Request, Response};

#[derive(Debug)]
pub struct RegistryRootOnly {
    prefix: String,
}

impl RegistryRootOnly {
    /// `base_path` as normalised by the starter: `/` or `/name`.
    #[must_use]
    pub fn new(base_path: &str) -> Self {
        Self {
            prefix: base_path.trim_end_matches('/').to_string(),
        }
    }
}

#[async_trait]
impl Middleware for RegistryRootOnly {
    async fn handle(&self, req: Request, next: &dyn Endpoint) -> Response {
        if is_registry_under_base(&self.prefix, req.uri().path()) {
            return Response::new(StatusCode::NOT_FOUND);
        }
        next.call(req).await
    }
}

/// True for `{prefix}/v2...` and `{prefix}/token...` when a base path is set.
#[must_use]
pub fn is_registry_under_base(prefix: &str, path: &str) -> bool {
    if prefix.is_empty() {
        return false;
    }
    let Some(rest) = path.strip_prefix(prefix) else {
        return false;
    };
    let rest = rest.trim_start_matches('/');
    rest.starts_with("v2") || rest.starts_with("token")
}
