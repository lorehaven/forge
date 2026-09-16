pub mod auth;
pub mod files;
pub mod home;
pub mod initializing;
pub mod projects;

pub fn register_routes() {
    auth::register_routes();
    files::register_routes();
    home::register_routes();
    initializing::register_routes();
    projects::register_routes();
}
