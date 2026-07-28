//! Parsing `FILE_STORAGES`.
//!
//! One malformed entry must not take the others with it: the operator finds out
//! from the log, and the storages that were configured correctly still serve.

use std::path::PathBuf;
use warehouse_service::routers::files::parse_storages;

#[test]
fn a_single_pair() {
    let storages = parse_storages("artifacts=/storage/artifacts");

    assert_eq!(storages.len(), 1);
    assert_eq!(storages[0].name, "artifacts");
    assert_eq!(storages[0].root, PathBuf::from("/storage/artifacts"));
}

#[test]
fn several_pairs_keep_their_order() {
    let storages = parse_storages("a=/one;b=/two;c=/three");

    let names: Vec<&str> = storages.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["a", "b", "c"]);
}

#[test]
fn whitespace_around_entries_is_ignored() {
    let storages = parse_storages("  a = /one ;  b=/two  ");

    assert_eq!(storages.len(), 2);
    assert_eq!(storages[0].name, "a");
    assert_eq!(storages[0].root, PathBuf::from("/one"));
    assert_eq!(storages[1].name, "b");
}

#[test]
fn an_empty_setting_configures_nothing() {
    assert!(parse_storages("").is_empty());
    assert!(parse_storages("   ").is_empty());
    assert!(parse_storages(";;").is_empty());
}

#[test]
fn a_trailing_separator_is_not_an_entry() {
    let storages = parse_storages("a=/one;");

    assert_eq!(storages.len(), 1);
}

#[test]
fn an_entry_with_no_equals_is_dropped_and_the_rest_survive() {
    let storages = parse_storages("broken;a=/one");

    assert_eq!(storages.len(), 1);
    assert_eq!(storages[0].name, "a");
}

#[test]
fn empty_halves_are_dropped() {
    let storages = parse_storages("=/one;b=;c=/three");

    assert_eq!(storages.len(), 1);
    assert_eq!(storages[0].name, "c");
}

#[test]
fn a_name_that_would_need_escaping_in_a_url_is_dropped() {
    // The name becomes a path segment. One containing a separator would make
    // `/api/v1/files/{storage}` ambiguous.
    let storages = parse_storages("we/ird=/one;ok-name_2=/two;also bad=/three");

    let names: Vec<&str> = storages.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["ok-name_2"]);
}

#[test]
fn a_repeated_name_does_not_shadow_the_first() {
    // Silently taking the last would make which directory is served depend on
    // the order of a semicolon-separated string.
    let storages = parse_storages("a=/one;a=/two");

    assert_eq!(storages.len(), 1);
    assert_eq!(storages[0].root, PathBuf::from("/one"));
}

#[test]
fn a_path_containing_an_equals_sign_survives() {
    // Split on the first `=` only; the rest is the path.
    let storages = parse_storages("a=/one=two");

    assert_eq!(storages.len(), 1);
    assert_eq!(storages[0].root, PathBuf::from("/one=two"));
}

#[test]
fn a_relative_root_is_kept_as_given() {
    // Resolved against the process's working directory, like the crates and
    // docker roots are. Not this function's business to reject.
    let storages = parse_storages("a=./storage/a");

    assert_eq!(storages[0].root, PathBuf::from("./storage/a"));
}
