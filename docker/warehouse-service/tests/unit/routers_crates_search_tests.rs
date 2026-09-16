use crate::support;
use http::{Method, StatusCode};
use quench_http::endpoint::Endpoint;
use support::WithCratesStorageRoot as WithStorageRoot;
use warehouse_service::routers::crates::search::{
    self, compare_versions, find_max_version, parse_semver,
};

fn publish(storage: &WithStorageRoot, name: &str, version: &str) {
    std::fs::create_dir_all(storage.dir.path().join(name).join(version)).unwrap();
}

// -----------------------------------------------------------------
// parse_semver / compare_versions
// -----------------------------------------------------------------

#[test]
fn parse_semver_reads_major_minor_patch() {
    assert_eq!(parse_semver("1.2.3").unwrap().0, 1);
    assert_eq!(parse_semver("1.2.3").unwrap().1, 2);
    assert_eq!(parse_semver("1.2.3").unwrap().2, 3);
}

#[test]
fn parse_semver_strips_build_metadata() {
    assert_eq!(
        parse_semver("1.2.3+build.5").unwrap(),
        (1, 2, 3, "\u{FFFF}".to_string())
    );
}

#[test]
fn parse_semver_rejects_malformed_versions() {
    assert!(parse_semver("1.2").is_none());
    assert!(parse_semver("a.b.c").is_none());
    assert!(parse_semver("1.2.3.4").is_none());
}

#[test]
fn compare_versions_orders_semver_numerically_not_lexicographically() {
    assert_eq!(
        compare_versions("1.2.0", "1.10.0"),
        std::cmp::Ordering::Less
    );
}

#[test]
fn compare_versions_places_a_release_above_its_prerelease() {
    assert_eq!(
        compare_versions("1.0.0-beta", "1.0.0"),
        std::cmp::Ordering::Less
    );
}

#[test]
fn compare_versions_falls_back_to_lexicographic_order_for_non_semver() {
    assert_eq!(compare_versions("latest", "stable"), "latest".cmp("stable"));
}

// -----------------------------------------------------------------
// find_max_version
// -----------------------------------------------------------------

#[tokio::test]
async fn find_max_version_is_none_without_any_version_directories() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(find_max_version(dir.path()).await, None);
}

#[tokio::test]
async fn find_max_version_picks_the_highest_semver_directory() {
    let dir = tempfile::tempdir().unwrap();
    for version in ["1.0.0", "2.0.0", "1.5.0"] {
        std::fs::create_dir(dir.path().join(version)).unwrap();
    }
    assert_eq!(
        find_max_version(dir.path()).await,
        Some("2.0.0".to_string())
    );
}

#[tokio::test]
async fn find_max_version_ignores_files_only_directories() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("owners.json"), b"[]").unwrap();
    std::fs::create_dir(dir.path().join("1.0.0")).unwrap();
    assert_eq!(
        find_max_version(dir.path()).await,
        Some("1.0.0".to_string())
    );
}

// -----------------------------------------------------------------
// handle
// -----------------------------------------------------------------

async fn app() -> (
    std::sync::Arc<dyn Endpoint>,
    std::sync::Arc<quench_http::di::Container>,
) {
    search::register_routes();
    let container = support::container_builder().build().await.unwrap();
    support::app(container).await
}

#[tokio::test]
async fn handle_rejects_an_empty_query() {
    let _storage = WithStorageRoot::new();
    let (app, container) = app().await;

    let resp = app
        .call(support::req(Method::GET, "/api/v1/crates?q=", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn handle_finds_crates_whose_name_contains_the_query_case_insensitively() {
    let storage = WithStorageRoot::new();
    publish(&storage, "my-http-client", "1.0.0");
    publish(&storage, "unrelated-crate", "1.0.0");
    publish(&storage, "index", "should-be-skipped");
    let (app, container) = app().await;

    let resp = app
        .call(support::req(
            Method::GET,
            "/api/v1/crates?q=HTTP",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body = support::json_body(resp).await;
    let crates = body["crates"].as_array().unwrap();
    assert_eq!(crates.len(), 1);
    assert_eq!(crates[0]["name"], "my-http-client");
    assert_eq!(crates[0]["max_version"], "1.0.0");
    assert_eq!(body["meta"]["total"], 1);
}

#[tokio::test]
async fn handle_paginates_results() {
    let storage = WithStorageRoot::new();
    for name in ["match-a", "match-b", "match-c"] {
        publish(&storage, name, "1.0.0");
    }
    let (app, container) = app().await;

    let resp = app
        .call(support::req(
            Method::GET,
            "/api/v1/crates?q=match&per_page=2&page=2",
            &container,
        ))
        .await;
    let body = support::json_body(resp).await;
    let crates = body["crates"].as_array().unwrap();
    assert_eq!(crates.len(), 1);
    assert_eq!(crates[0]["name"], "match-c");
    assert_eq!(body["meta"]["total"], 3);
}

#[tokio::test]
async fn handle_skips_a_crate_directory_with_no_version_subdirectories() {
    let storage = WithStorageRoot::new();
    std::fs::create_dir_all(storage.dir.path().join("match-empty")).unwrap();
    let (app, container) = app().await;

    let resp = app
        .call(support::req(
            Method::GET,
            "/api/v1/crates?q=match",
            &container,
        ))
        .await;
    let body = support::json_body(resp).await;
    assert_eq!(body["meta"]["total"], 0);
}
