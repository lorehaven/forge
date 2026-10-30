pub mod auth;
pub mod client;

pub fn register_routes() {
    auth::register_routes();
    client::register_routes();
}
