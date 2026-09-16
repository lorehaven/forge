pub mod cancel_upload;
pub mod check_exists;
pub mod complete_upload;
pub mod get_upload_status;
pub mod retrieve;
pub mod start_upload;
pub mod upload_chunk;

pub fn register_routes() {
    cancel_upload::register_routes();
    check_exists::register_routes();
    complete_upload::register_routes();
    get_upload_status::register_routes();
    retrieve::register_routes();
    start_upload::register_routes();
    upload_chunk::register_routes();
}
