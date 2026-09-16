//! The artifact registry's management UI: browse programs/platforms/versions,
//! yank/unyank with `warehouse:write`. Mirrors `super::crates`'s tree layout.

pub mod catalog;

pub fn register_routes() {
    catalog::register_routes();
}
