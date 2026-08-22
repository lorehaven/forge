//! Coverage for the file-based/pure halves of `image_updates.rs` -
//! `discover_images`/`apply_updates` (real filesystem, no network) and
//! `find_updates`/`print_rows`/`print_results` (pure data, or exercised only
//! through branches - like a floating-tag skip - that never reach the
//! network). The registry-talking parts (`registry_v2_tags`, `docker_hub_tags`,
//! `bearer_token`) already have their own coverage via `wiremock` in
//! `image_updates_tests.rs`.

use riveter::image_updates::{
    ImageOccurrence, ImageRef, RegistryAuth, ScanMessage, apply_updates, discover_images,
    find_updates, print_results, print_rows,
};
use std::fs;

#[test]
fn discover_images_finds_image_lines_across_nested_templates_and_skips_bad_refs() {
    let dir = tempfile::tempdir().unwrap();
    let nested = dir.path().join("prod");
    fs::create_dir_all(&nested).unwrap();

    fs::write(
        dir.path().join("base.yaml.j2"),
        "spec:\n  image: nginx:1.27\n",
    )
    .unwrap();
    fs::write(
        nested.join("app.yaml.j2"),
        "spec:\n  image: registry.example.com/team/app:2.0.1\n  # not an image line\n",
    )
    .unwrap();
    // Not a `.yaml.j2` file - should be ignored entirely.
    fs::write(dir.path().join("notes.txt"), "image: nginx:1.27\n").unwrap();

    let occurrences = discover_images(dir.path()).unwrap();
    assert_eq!(occurrences.len(), 2);
    assert!(
        occurrences
            .iter()
            .any(|o| o.image.repository == "library/nginx" || o.image.original == "nginx:1.27")
    );
    assert!(
        occurrences
            .iter()
            .any(|o| o.image.original.contains("team/app"))
    );
}

#[test]
fn discover_images_is_empty_for_a_directory_with_no_templates() {
    let dir = tempfile::tempdir().unwrap();
    let occurrences = discover_images(dir.path()).unwrap();
    assert!(occurrences.is_empty());
}

fn occurrence(path: &std::path::Path, original: &str, tag: &str) -> ImageOccurrence {
    ImageOccurrence {
        path: path.to_path_buf(),
        line_number: 2,
        image: ImageRef {
            original: original.to_string(),
            registry: "registry-1.docker.io".to_string(),
            repository: "library/nginx".to_string(),
            tag: tag.to_string(),
        },
    }
}

#[test]
fn apply_updates_rewrites_only_the_matching_image_line() {
    let dir = tempfile::tempdir().unwrap();
    let template = dir.path().join("app.yaml.j2");
    fs::write(
        &template,
        "spec:\n  image: nginx:1.26\n  other: unrelated\n",
    )
    .unwrap();

    let updates = vec![riveter::image_updates::UpdateCandidate {
        occurrence: occurrence(&template, "nginx:1.26", "1.26"),
        newest_tag: "1.27".to_string(),
    }];

    apply_updates(&updates).unwrap();

    let rewritten = fs::read_to_string(&template).unwrap();
    assert!(rewritten.contains("nginx:1.27"), "{rewritten}");
    assert!(!rewritten.contains("1.26"), "{rewritten}");
    assert!(rewritten.contains("other: unrelated"));
    assert!(rewritten.ends_with('\n'));
}

#[test]
fn apply_updates_with_no_candidates_is_a_no_op() {
    apply_updates(&[]).unwrap();
}

#[test]
fn find_updates_with_no_occurrences_returns_nothing() {
    let (updates, messages) = find_updates(Vec::new(), &RegistryAuth::default());
    assert!(updates.is_empty());
    assert!(messages.is_empty());
}

#[test]
fn find_updates_skips_a_floating_tag_without_any_network_call() {
    let dir = tempfile::tempdir().unwrap();
    let occ = occurrence(&dir.path().join("app.yaml.j2"), "nginx:latest", "latest");

    let (updates, messages) = find_updates(vec![occ], &RegistryAuth::default());
    assert!(updates.is_empty());
    assert_eq!(messages.len(), 1);
    match &messages[0] {
        ScanMessage::Skip { detail, .. } => assert!(detail.contains("latest")),
        ScanMessage::Error { .. } => panic!("expected a Skip message"),
    }
}

#[test]
fn print_rows_and_print_results_do_not_panic_on_empty_or_populated_input() {
    print_rows("Title", &[]);
    print_rows(
        "Title",
        &[[
            "update".to_string(),
            "a:1".to_string(),
            "b".to_string(),
            "c".to_string(),
        ]],
    );
    print_results(&[], &[]);
}
