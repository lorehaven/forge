//! The file storage management UI: [`storages`] lists/provisions/deletes
//! dynamic storages, [`browse`] is the per-storage file browser. Static storages are read-only.

pub mod browse;
pub mod storages;

pub fn register_routes() {
    browse::register_routes();
    storages::register_routes();
}
