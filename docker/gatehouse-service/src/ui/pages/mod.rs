pub mod account;
pub mod admin;
pub mod auth;
pub mod confirm_email;
pub mod home;
pub mod invite;
pub mod register;
pub mod resend;
pub mod reset;
pub mod unsubscribe;

pub(super) fn register_routes() {
    account::register_routes();
    admin::register_routes();
    auth::register_routes();
    confirm_email::register_routes();
    home::register_routes();
    invite::register_routes();
    register::register_routes();
    resend::register_routes();
    reset::register_routes();
    unsubscribe::register_routes();
}
