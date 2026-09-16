pub mod auth;
pub mod home;
pub mod models;
pub mod vllm;

pub(super) fn register_routes() {
    auth::register_routes();
    home::register_routes();
    models::register_routes();
    vllm::register_routes();
}
