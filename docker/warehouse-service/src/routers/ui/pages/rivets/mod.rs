//! The rivet registry's management UI: browse packages and their versions, yank/unyank with
//! `warehouse:write`. Mirrors `super::artifacts`'s tree layout, one level shallower - a package
//! has versions but no platforms.

pub mod catalog;

pub fn register_routes() {
    catalog::register_routes();
}
