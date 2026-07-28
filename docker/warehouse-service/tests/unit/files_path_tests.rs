//! What a caller is allowed to name.
//!
//! This is the whole attack surface of the files API: everything else in the
//! module trusts that whatever came back from `relative` stays inside the
//! storage.

use std::path::PathBuf;
use warehouse_service::routers::files::{PathError, relative};

fn accepted(path: &str) -> PathBuf {
    relative(path).unwrap_or_else(|why| panic!("`{path}` should be accepted, got {why:?}"))
}

fn refused(path: &str) -> PathError {
    match relative(path) {
        Ok(resolved) => panic!("`{path}` should be refused, got `{}`", resolved.display()),
        Err(why) => why,
    }
}

#[test]
fn plain_names_are_kept_as_written() {
    assert_eq!(accepted("thing"), PathBuf::from("thing"));
    assert_eq!(
        accepted("conveyor/run-1/thing.tar.gz"),
        PathBuf::from("conveyor/run-1/thing.tar.gz")
    );
}

#[test]
fn a_leading_dot_slash_is_dropped_rather_than_refused() {
    assert_eq!(accepted("./thing"), PathBuf::from("thing"));
    assert_eq!(accepted("./a/./b"), PathBuf::from("a/b"));
}

#[test]
fn parent_components_are_refused_wherever_they_appear() {
    assert_eq!(refused(".."), PathError::Traversal);
    assert_eq!(refused("../etc/passwd"), PathError::Traversal);
    assert_eq!(refused("a/../b"), PathError::Traversal);
    assert_eq!(refused("a/b/.."), PathError::Traversal);
    // The one that catches a normaliser: this collapses to `b` lexically, so
    // an implementation that resolves rather than refuses would accept it and
    // then have to be right about what `a` is.
    assert_eq!(refused("a/../../a/b"), PathError::Traversal);
}

#[test]
fn absolute_paths_are_refused() {
    assert_eq!(refused("/etc/passwd"), PathError::Absolute);
    assert_eq!(refused("/"), PathError::Absolute);
}

#[test]
fn an_empty_path_is_refused_rather_than_meaning_the_root() {
    assert_eq!(refused(""), PathError::Empty);
    assert_eq!(refused("   "), PathError::Empty);
    // Spells the storage root, which is not a file anyone can write.
    assert_eq!(refused("."), PathError::Empty);
    assert_eq!(refused("./"), PathError::Empty);
}

#[test]
fn nul_and_control_bytes_are_refused() {
    // The interesting one: everything before the NUL passes any textual check,
    // and the syscall then opens `thing`.
    assert_eq!(refused("thing\0.txt"), PathError::Invalid);
    assert_eq!(refused("thing\n.txt"), PathError::Invalid);
    assert_eq!(refused("thing\r\n"), PathError::Invalid);
    assert_eq!(refused("thing\x7f"), PathError::Invalid);
}

#[test]
fn names_that_merely_look_alarming_are_allowed() {
    // `..` is a path component, not a substring. A file legitimately called
    // `..thing` or `a..b` is not traversal and refusing it would be wrong.
    assert_eq!(accepted("..thing"), PathBuf::from("..thing"));
    assert_eq!(accepted("a..b"), PathBuf::from("a..b"));
    assert_eq!(accepted("...."), PathBuf::from("...."));
}

#[test]
fn spaces_and_unicode_survive() {
    assert_eq!(accepted("a file.txt"), PathBuf::from("a file.txt"));
    assert_eq!(accepted("ünïcødé/ok"), PathBuf::from("ünïcødé/ok"));
}

#[test]
fn repeated_separators_collapse() {
    assert_eq!(accepted("a//b"), PathBuf::from("a/b"));
    assert_eq!(accepted("a///b//c"), PathBuf::from("a/b/c"));
}

#[test]
fn a_trailing_separator_does_not_change_the_target() {
    assert_eq!(accepted("a/b/"), PathBuf::from("a/b"));
}
