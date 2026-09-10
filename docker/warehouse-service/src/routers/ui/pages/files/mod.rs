//! The file storage management UI, in two pages:
//!
//! - [`storages`] lists the storages this deployment serves and - for a caller
//!   holding `warehouse:write` or a wildcard role - provisions, reconfigures
//!   and deletes a dynamic storage.
//! - [`browse`] is the per-storage file browser: a navigable tree with an
//!   in-place preview pane, and the one mutating control left here (delete a
//!   single file, [`storages::delete_file`]).
//!
//! Static (`FILE_STORAGES`) storages appear read-only: the operator owns
//! their layout, so there is nothing here to change about them. Everything
//! mutating is held to [`crate::routers::ui::authz::require_manage`], the same
//! bar `routers::files::ops::storages` enforces on the JSON API.

pub mod browse;
pub mod storages;
