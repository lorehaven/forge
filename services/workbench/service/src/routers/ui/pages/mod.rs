pub mod auth;
pub mod client;

pub(super) fn register_routes() {
    auth::register_routes();
    client::register_routes();
}
