pub mod artifacts;
pub mod auth;
pub mod crates;
pub mod docker;
pub mod files;
pub mod home;

pub fn register_routes() {
    artifacts::register_routes();
    auth::register_routes();
    crates::register_routes();
    docker::register_routes();
    files::register_routes();
    home::register_routes();
}
