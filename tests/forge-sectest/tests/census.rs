//! The route census must see every route, or the auth matrix has silent blind spots.

// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports)]

use forge_sectest::census::{
    Piece, locked_route_sources, parse_source, pieces, quench_route_sources, render, sample_value,
    scan_forge,
};
use std::fs;
use std::path::PathBuf;

fn forge_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[quench_test::test]
async fn attributes_become_routes() {
    let source = r#"
#[get("/api/v1/things/{id}")]
pub async fn one() {}

    #[post("/api/v1/things")]
pub async fn create() {}

// #[get("/commented/out")] is not a route
#[delete("/v2/{name:.+}/blobs/{digest}")]
pub async fn remove() {}

#[derive(Debug)]
#[routes_not_a_method("/x")]
struct NotARoute;
"#;
    let routes = parse_source("svc", source, "x.rs");
    let keys: Vec<String> = routes
        .iter()
        .map(forge_sectest::census::Route::key)
        .collect();
    assert_eq!(
        keys,
        vec![
            "svc GET /api/v1/things/{id}",
            "svc POST /api/v1/things",
            "svc DELETE /v2/{name:.+}/blobs/{digest}",
        ]
    );
    assert!(routes[1].is_mutating() && !routes[0].is_mutating());
}

#[quench_test::test]
async fn patterns_split_into_literals_and_parameters() {
    let parts = pieces("/v2/{name:.+}/manifests/{reference}");
    assert_eq!(
        parts,
        vec![
            Piece::Literal("/v2/".into()),
            Piece::Param {
                name: "name".into(),
                constraint: Some(".+".into())
            },
            Piece::Literal("/manifests/".into()),
            Piece::Param {
                name: "reference".into(),
                constraint: None
            },
        ]
    );
    // Regexes may contain braces of their own.
    let parts = pieces("/x/{id:[0-9]{3}}/y");
    assert_eq!(parts.len(), 3, "{parts:?}");
}

#[quench_test::test]
async fn sample_values_are_inert_and_replacement_targets_one_parameter() {
    assert_eq!(
        sample_value("project_id"),
        "00000000-0000-0000-0000-000000000000"
    );
    assert_eq!(sample_value("version"), "0.0.0-sectest");
    assert!(sample_value("digest").starts_with("sha256:"));
    let parts = pieces("/a/{x}/b/{y}");
    assert_eq!(render(&parts, None), "/a/sectest/b/sectest");
    assert_eq!(render(&parts, Some((3, "../evil"))), "/a/sectest/b/../evil");
}

#[quench_test::test]
async fn the_census_matches_a_naive_count_of_route_attributes() {
    // Independent of the parser: count attribute lines per service with plain string matching. If a
    // new attribute style (multi-line, new verb) slips past the census, these numbers diverge.
    let root = forge_root();
    let routes = scan_forge(&root, &[]);
    assert!(
        routes.len() > 300,
        "only {} routes found; wrong root {}?",
        routes.len(),
        root.display()
    );

    for entry in fs::read_dir(root.join("docker")).unwrap().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(service) = name.strip_suffix("-service") else {
            continue;
        };
        let mut naive = 0;
        let mut stack = vec![entry.path().join("src")];
        while let Some(dir) = stack.pop() {
            for item in fs::read_dir(&dir).into_iter().flatten().flatten() {
                let path = item.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    let text = fs::read_to_string(&path).unwrap_or_default();
                    naive += text
                        .lines()
                        .filter(|l| {
                            let l = l.trim_start();
                            ["get", "post", "put", "delete", "patch"]
                                .iter()
                                .any(|m| l.starts_with(&format!("#[{m}(")))
                        })
                        .count();
                }
            }
        }
        let found = routes.iter().filter(|r| r.service == service).count();
        assert_eq!(
            found, naive,
            "{service}: census found {found}, a naive count found {naive}"
        );
    }
}

#[quench_test::test]
async fn the_shared_quench_routes_are_found_at_the_locked_versions_without_a_sibling_checkout() {
    // CI has no `../quench`, and a job that compiles only this crate never downloaded these two. Asking
    // cargo for the workspace's locked copies is what works there - tested directly, not through the
    // sibling shortcut that happens to answer on a developer's machine.
    let found = locked_route_sources(&forge_root());
    assert_eq!(
        found.len(),
        2,
        "quench-starter and quench-auth should both resolve: {found:?}"
    );
    let routes = found
        .iter()
        .flat_map(|dir| forge_sectest::census::scan_dir("probe", dir))
        .count();
    assert!(
        routes >= 5,
        "found sources {found:?} but only {routes} routes in them"
    );
}

#[quench_test::test]
async fn with_no_environment_or_sibling_to_help_the_lookup_still_finds_them() {
    // A root with no `../quench` beside it and no override: the full chain, as in CI.
    let found = quench_route_sources(std::path::Path::new("/nonexistent/forge"));
    assert!(
        !found.is_empty(),
        "no quench-starter/quench-auth sources found anywhere"
    );
}

quench_test::main!();
