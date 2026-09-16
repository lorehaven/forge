pub mod catalog;
pub mod check;
pub mod storage;
pub mod tags;

pub fn register_routes() {
    catalog::register_routes();
    check::register_routes();
    tags::register_routes();
}
