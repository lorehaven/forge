use crate::routers::vllm::engine::VllmEngine;
use actix_web::dev::HttpServiceFactory;
use actix_web::web;
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

pub fn scope(engine: Arc<dyn VllmEngine>) -> impl HttpServiceFactory {
    web::scope("/api/v1/models")
        .app_data(web::Data::new(engine))
        .service(list::handle_list)
        .service(list::handle_grid)
        .service(list::estimates_modal)
        .service(list::empty_estimates_modal_endpoint)
        .service(list::delete_modal)
        .service(list::empty_delete_modal_endpoint)
        .service(delete::delete_model)
        .service(delete::delete_model_form)
        .service(running::list_running_models)
}
