//! End-to-end coverage for [`generate_manifests_selected`] and friends -
//! everything render.rs does after `render_to_string`'s lower-level template
//! render, up through writing files under a real (temp) `manifests/` output
//! directory. None of this touches kubectl: `kube_context` only reads a field
//! out of the overlay data, it never shells out - see its own doc comment.

use crate::env_support::cwd_lock;
use riveter::render::{
    ResourceScope, Selector, generate_manifests, generate_manifests_selected,
    generate_manifests_with_scope, list_resources,
};
use std::fs;

const OVERLAY: &str = r"
namespace_name: golden

resources:
  - kind: deployment
    name: api
    image: nginx

  - kind: deployment
    name: worker
    image: nginx
    immutable: true

  - kind: secret
    name: db-creds
    data:
      password: hunter2
";

/// Runs `body` in a fresh temp cwd with `overlays/<env>/overlay.yaml` set to
/// [`OVERLAY`] - same locking discipline as `env_tests`' own helper, since
/// cwd is process-global and every test in this binary shares it.
fn in_temp_cwd_with_overlay<T>(env: &str, body: impl FnOnce() -> T) -> T {
    let _guard = cwd_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let dir = tempfile::tempdir().unwrap();
    let overlay_dir = dir.path().join("overlays").join(env);
    fs::create_dir_all(&overlay_dir).unwrap();
    fs::write(overlay_dir.join("overlay.yaml"), OVERLAY).unwrap();

    let original = std::env::current_dir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    let result = body();

    std::env::set_current_dir(original).unwrap();
    result
}

#[test]
fn generate_manifests_writes_every_resource_to_the_default_output_path() {
    in_temp_cwd_with_overlay("golden", || {
        let path = generate_manifests("golden").unwrap();
        assert_eq!(path, "manifests/golden-manifests.yaml");
        let contents = fs::read_to_string(&path).unwrap();
        assert!(contents.contains("name: api"));
        assert!(contents.contains("name: worker"));
        assert!(contents.contains("name: db-creds"));
    });
}

#[test]
fn generate_manifests_with_scope_mutable_excludes_the_immutable_resource() {
    in_temp_cwd_with_overlay("golden", || {
        let rendered = generate_manifests_with_scope("golden", ResourceScope::Mutable).unwrap();
        let names: Vec<&str> = rendered.selected.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&"api"));
        assert!(!names.contains(&"worker"), "{names:?}");
    });
}

#[test]
fn generate_manifests_with_scope_immutable_selects_only_the_immutable_resource() {
    in_temp_cwd_with_overlay("golden", || {
        let rendered = generate_manifests_with_scope("golden", ResourceScope::Immutable).unwrap();
        let names: Vec<&str> = rendered.selected.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["worker"]);
    });
}

#[test]
fn generate_manifests_selected_reports_a_secret_as_sensitive_and_locks_the_file_down() {
    in_temp_cwd_with_overlay("golden", || {
        let selector = riveter::render::Selector::parse(&["secret/db-creds"]).unwrap();
        let rendered =
            generate_manifests_selected("golden", ResourceScope::All, &selector).unwrap();
        assert_eq!(rendered.resource_count, 1);
        assert_eq!(rendered.path, "manifests/golden-manifests.selection.yaml");
        assert!(!rendered.skipped_out_of_scope.is_empty());

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&rendered.path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600, "secret manifest should be owner-only");
        }
    });
}

#[test]
fn generate_manifests_selected_with_an_unmatched_selector_errors() {
    in_temp_cwd_with_overlay("golden", || {
        let selector = Selector::parse(&["deployment/does-not-exist"]).unwrap();
        let err = generate_manifests_selected("golden", ResourceScope::All, &selector).unwrap_err();
        assert!(err.to_string().to_lowercase().contains("does-not-exist"));
    });
}

#[test]
fn list_resources_lists_every_resource_regardless_of_scope() {
    in_temp_cwd_with_overlay("golden", || {
        let resources = list_resources("golden").unwrap();
        assert_eq!(resources.len(), 3);
        assert!(resources.iter().any(|r| r.name == "api"));
        assert!(resources.iter().any(|r| r.name == "worker"));
        assert!(resources.iter().any(|r| r.name == "db-creds"));
    });
}

#[test]
fn generate_manifests_errors_for_a_missing_overlay() {
    in_temp_cwd_with_overlay("golden", || {
        let err = generate_manifests("no-such-env").unwrap_err();
        assert!(!err.to_string().is_empty());
    });
}
