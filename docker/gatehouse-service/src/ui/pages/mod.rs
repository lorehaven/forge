pub mod account;
pub mod admin;
pub mod auth;
pub mod home;
pub mod register;
pub mod reset;

pub(super) fn register_routes() {
    account::register_routes();
    admin::register_routes();
    auth::register_routes();
    home::register_routes();
    register::register_routes();
    reset::register_routes();
}
