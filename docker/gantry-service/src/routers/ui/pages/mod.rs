pub mod apps;
pub mod auth;
pub mod deployments;
pub mod home;
pub mod operations;
pub mod plans;
pub mod resources;
pub mod targets;

pub(super) fn register_routes() {
    apps::register_routes();
    auth::register_routes();
    deployments::register_routes();
    home::register_routes();
    targets::register_routes();
    plans::register_routes();
    resources::register_routes();
    operations::register_routes();
}
