pub mod catalog;
pub mod tags;

pub fn register_routes() {
    catalog::register_routes();
    tags::register_routes();
}
