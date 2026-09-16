use quench_auth::domain::jwt::JwtConfig;
use quench_auth::http::middleware::auth::Auth;
use quench_http::prelude::{Endpoint, OnPathPrefix, wrap};
use serde::Deserialize;
use std::sync::Arc;

pub mod delete;
pub mod discovery;
pub mod list;
pub mod mod_impl;
pub mod running;
pub mod store;
pub mod sync;
pub mod types;

pub use mod_impl::{GGUF_ROOTS, HF_ROOTS};
pub use store::{init_model_store, warm_model_cache};
pub use sync::start_sync_job;
pub use types::*;

#[derive(Debug, Deserialize)]
pub struct VllmArchitecturesFile {
    pub architectures: Vec<String>,
}

/// No `RequireWrite`: `delete_model`/`delete_model_form` gate via `mod_impl::can`.
/// `base_path` matters - `OnPathPrefix` sees the raw un-mounted path.
pub fn wrap_auth(
    app: Arc<dyn Endpoint>,
    jwt_config: JwtConfig,
    base_path: &str,
) -> Arc<dyn Endpoint> {
    let prefix: &'static str = Box::leak(format!("{base_path}/api/v1/models").into_boxed_str());
    wrap(app, OnPathPrefix::new(prefix, Auth::new(jwt_config)))
}

pub fn register_routes() {
    let _ = list::handle_list as fn(_) -> _;
    let _ = list::handle_grid as fn(_, _, _) -> _;
    let _ = list::estimates_modal as fn(_) -> _;
    let _ = list::empty_estimates_modal_endpoint as fn() -> _;
    let _ = list::delete_modal as fn(_) -> _;
    let _ = list::empty_delete_modal_endpoint as fn() -> _;
    let _ = delete::delete_model as fn(_, _, _) -> _;
    let _ = delete::delete_model_form as fn(_, _, _) -> _;
    let _ = running::list_running_models as fn(_, _, _) -> _;
}
