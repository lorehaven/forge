pub mod auth;
pub mod client;
pub mod player;

pub(super) fn register_routes() {
    auth::register_routes();
    client::register_routes();
    player::register_routes();
}
