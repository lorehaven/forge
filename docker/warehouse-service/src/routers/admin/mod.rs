pub mod crates;
pub mod docker;

pub fn register_routes() {
    crates::register_routes();
    docker::register_routes();
}
