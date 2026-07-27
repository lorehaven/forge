//! Golden-file tests for the templates.
//!
//! Each `tests/golden/<name>.overlay.yaml` is rendered and compared against
//! `<name>.expected.yaml`. Parsing a template proves only that it is valid
//! Jinja; these pin what it actually emits.
//!
//! Run with `UPDATE_GOLDEN=1` to rewrite the expectations after an intentional
//! change, then read the diff before committing it.

use riveter::render::{check_embedded_templates, render_to_string};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

const ENV_NAME: &str = "golden";

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

/// The variables the fixtures reference. Fixed here so a render is reproducible.
fn fixture_vars() -> HashMap<String, String> {
    [
        ("IMAGE_TAG", "1.27"),
        ("DB_PASSWORD", "hunter2"),
        ("NAMESPACE", "golden"),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect()
}

fn fixtures() -> Vec<(String, PathBuf)> {
    let mut found: Vec<(String, PathBuf)> = fs::read_dir(golden_dir())
        .expect("tests/golden should exist")
        .map(|e| e.expect("readable entry").path())
        .filter_map(|path| {
            let name = path.file_name()?.to_str()?.strip_suffix(".overlay.yaml")?;
            Some((name.to_string(), path.clone()))
        })
        .collect();

    found.sort();
    assert!(!found.is_empty(), "no golden fixtures found");
    found
}

fn render_fixture(path: &Path) -> String {
    let src = fs::read_to_string(path).expect("fixture should be readable");

    render_to_string(ENV_NAME, &src, &fixture_vars())
        .unwrap_or_else(|e| panic!("rendering {} failed: {e:#}", path.display()))
}

#[test]
fn fixtures_render_to_their_golden_output() {
    let update = std::env::var_os("UPDATE_GOLDEN").is_some();
    let mut stale = Vec::new();

    for (name, path) in fixtures() {
        let actual = render_fixture(&path);
        let expected_path = golden_dir().join(format!("{name}.expected.yaml"));

        if update {
            fs::write(&expected_path, &actual).expect("golden should be writable");
            continue;
        }

        let expected = fs::read_to_string(&expected_path).unwrap_or_else(|_| {
            panic!(
                "missing {}; run with UPDATE_GOLDEN=1 to create it",
                expected_path.display()
            )
        });

        if actual != expected {
            stale.push(format!(
                "--- {name} ---\n{}",
                first_difference(&expected, &actual)
            ));
        }
    }

    assert!(
        stale.is_empty(),
        "rendered output changed:\n\n{}\n\nre-run with UPDATE_GOLDEN=1 if this is intended",
        stale.join("\n\n")
    );
}

/// The first differing line with its neighbours, so a failure names the change
/// instead of dumping two whole manifests.
fn first_difference(expected: &str, actual: &str) -> String {
    let exp: Vec<&str> = expected.lines().collect();
    let act: Vec<&str> = actual.lines().collect();

    for (i, (e, a)) in exp.iter().zip(act.iter()).enumerate() {
        if e != a {
            return format!("line {}:\n  expected: {e}\n  actual:   {a}", i + 1);
        }
    }

    format!(
        "line count differs: expected {} lines, got {}",
        exp.len(),
        act.len()
    )
}

#[test]
fn every_rendered_document_is_valid_yaml() {
    for (name, path) in fixtures() {
        let rendered = render_fixture(&path);

        for (i, doc) in rendered.split("\n---\n").enumerate() {
            let parsed: serde_yaml::Value = serde_yaml::from_str(doc)
                .unwrap_or_else(|e| panic!("{name} document {i} is not valid YAML: {e}\n\n{doc}"));

            // Anything Kubernetes will accept has at least these two.
            assert!(
                parsed.get("apiVersion").is_some(),
                "{name} document {i} has no apiVersion:\n{doc}"
            );
            assert!(
                parsed.get("kind").is_some(),
                "{name} document {i} has no kind:\n{doc}"
            );
        }
    }
}

#[test]
fn every_template_is_covered_by_a_fixture() {
    // Without this, a new kind can ship with no rendering test at all.
    let templates = check_embedded_templates().expect("templates should parse");

    let rendered: String = fixtures()
        .iter()
        .map(|(_, path)| render_fixture(path))
        .collect::<Vec<_>>()
        .join("\n");
    let sources: String = fixtures()
        .iter()
        .map(|(_, path)| fs::read_to_string(path).expect("readable"))
        .collect::<Vec<_>>()
        .join("\n");

    let uncovered: Vec<&str> = templates
        .iter()
        .copied()
        .filter(|t| !t.starts_with('_'))
        .filter(|t| {
            let kind = t.trim_end_matches(".yaml.j2");
            // A kind counts as covered when a fixture declares it, either by
            // its canonical name or by an alias riveter resolves.
            !sources.contains(&format!("kind: {kind}"))
                && !rendered.to_lowercase().contains(&format!("kind: {kind}"))
        })
        .collect();

    assert!(
        uncovered.is_empty(),
        "these templates have no golden fixture: {uncovered:?}"
    );
}
