pub mod auth;
pub mod home;
pub mod issues;
pub mod projects;

pub(super) fn register_routes() {
    auth::register_routes();
    home::register_routes();
    issues::register_routes();
    projects::register_routes();
}
