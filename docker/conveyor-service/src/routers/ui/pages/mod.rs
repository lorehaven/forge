pub mod auth;
pub mod credentials;
pub mod home;
pub mod jobs;
pub mod pipelines;
pub mod repos;
pub mod runs;
pub mod scan;
pub mod shared;

pub fn register_routes() {
    auth::register_routes();
    credentials::register_routes();
    home::register_routes();
    jobs::register_routes();
    pipelines::register_routes();
    repos::register_routes();
    runs::register_routes();
    scan::register_routes();
}
