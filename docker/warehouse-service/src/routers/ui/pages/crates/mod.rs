pub mod catalog;
pub mod storage;

pub fn register_routes() {
    catalog::register_routes();
}
