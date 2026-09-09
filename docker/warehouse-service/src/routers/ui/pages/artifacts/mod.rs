//! The artifact registry's management UI: browse published programs, their
//! platforms and versions, and (for a caller with `warehouse:write`) yank or
//! unyank a version. Mirrors `super::crates` - a tree on the left, a version's
//! metadata and its one mutating action on the right.

pub mod catalog;
