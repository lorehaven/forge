//! The file storage management UI: list the storages this deployment serves,
//! browse a storage's contents, and - for a caller holding `warehouse:write`
//! or a wildcard role - provision, reconfigure and delete a dynamic storage,
//! or delete a single file.
//!
//! Static (`FILE_STORAGES`) storages appear read-only: the operator owns
//! their layout, so there is nothing here to change about them. Everything
//! mutating is held to [`crate::routers::ui::authz::require_manage`], the same
//! bar `routers::files::ops::storages` enforces on the JSON API.

pub mod storages;
