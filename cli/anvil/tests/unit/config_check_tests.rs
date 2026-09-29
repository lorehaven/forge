use anvil::commands::config_check::validate;

use crate::support;
use support::stable_cwd_lock;

#[test]
fn a_minimal_valid_config_has_no_issues() {
    let toml = r#"
[docker.modules.core]
packages = ["service"]
dockerfile = "Dockerfile"

[install]
packages = ["service"]

[release]
registry = "acme"
packages = ["service"]
"#;
    assert_eq!(validate(toml).expect("parses"), Vec::new());
}

#[test]
fn an_empty_file_has_no_issues() {
    // Every top-level section is `#[serde(default)]` - an empty file is a
    // config with nothing configured, not a malformed one.
    assert_eq!(validate("").expect("parses"), Vec::new());
}

#[test]
fn malformed_toml_syntax_is_a_parse_error() {
    assert!(validate("not [ valid toml").is_err());
}

#[test]
fn an_unknown_top_level_field_is_reported() {
    let issues = validate("bogus = true\n").expect("parses");
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].path, "bogus");
    assert!(issues[0].message.contains("unknown field"));
}

#[test]
fn an_unknown_docker_field_is_reported() {
    let issues = validate("[docker]\nbogus = 1\n").expect("parses");
    assert!(
        issues
            .iter()
            .any(|i| i.path == "docker.bogus" && i.message.contains("unknown field"))
    );
}

#[test]
fn a_module_missing_packages_and_dockerfile_reports_both() {
    let issues = validate("[docker.modules.core]\n").expect("parses");
    assert_eq!(
        issues
            .iter()
            .filter(|i| i.message.contains("missing required field"))
            .count(),
        2,
        "{issues:?}"
    );
    assert!(
        issues
            .iter()
            .any(|i| i.path == "docker.modules.core" && i.message.contains("`packages`"))
    );
    assert!(
        issues
            .iter()
            .any(|i| i.path == "docker.modules.core" && i.message.contains("`dockerfile`"))
    );
}

#[test]
fn an_empty_packages_list_is_reported() {
    let toml = "[docker.modules.core]\npackages = []\ndockerfile = \"Dockerfile\"\n";
    let issues = validate(toml).expect("parses");
    assert!(issues.iter().any(
        |i| i.path == "docker.modules.core.packages" && i.message.contains("must not be empty")
    ));
}

#[test]
fn an_empty_dockerfile_is_reported() {
    let toml = "[docker.modules.core]\npackages = [\"service\"]\ndockerfile = \"\"\n";
    let issues = validate(toml).expect("parses");
    assert!(
        issues
            .iter()
            .any(|i| i.path == "docker.modules.core.dockerfile"
                && i.message.contains("must not be empty"))
    );
}

#[test]
fn an_override_for_a_package_not_in_this_modules_list_is_reported() {
    let toml = r#"
[docker.modules.core]
packages = ["service"]
dockerfile = "Dockerfile"

[docker.modules.core.other-service]
image_name = "other"
"#;
    let issues = validate(toml).expect("parses");
    assert!(issues.iter().any(|i| {
        i.path == "docker.modules.core.other-service" && i.message.contains("not in this module's")
    }));
}

#[test]
fn an_unknown_override_field_is_reported() {
    let toml = r#"
[docker.modules.core]
packages = ["service"]
dockerfile = "Dockerfile"

[docker.modules.core.service]
bogus = "x"
"#;
    let issues = validate(toml).expect("parses");
    assert!(
        issues
            .iter()
            .any(|i| i.path == "docker.modules.core.service.bogus"
                && i.message.contains("unknown field"))
    );
}

#[test]
fn setting_both_registry_and_registries_on_an_override_is_reported() {
    let toml = r#"
[docker.modules.core]
packages = ["service"]
dockerfile = "Dockerfile"

[docker.modules.core.service]
registry = "acme"
registries = ["acme"]
"#;
    let issues = validate(toml).expect("parses");
    assert!(
        issues
            .iter()
            .any(|i| i.path == "docker.modules.core.service" && i.message.contains("deprecated"))
    );
}

#[test]
fn a_package_built_by_two_modules_is_reported() {
    let toml = r#"
[docker.modules.a]
packages = ["service"]
dockerfile = "Dockerfile"

[docker.modules.b]
packages = ["service"]
dockerfile = "Dockerfile"
"#;
    let issues = validate(toml).expect("parses");
    assert!(
        issues
            .iter()
            .any(|i| i.message.contains("also built by module")),
        "{issues:?}"
    );
}

#[test]
fn a_non_string_entry_in_an_array_is_reported_by_position() {
    let toml = "[install]\npackages = [\"ok\", 1]\n";
    let issues = validate(toml).expect("parses");
    assert!(
        issues
            .iter()
            .any(|i| i.path == "install.packages[1]" && i.message.contains("expected a string"))
    );
}

#[test]
fn a_non_string_build_arg_value_is_reported() {
    let toml = r#"
[docker.modules.core]
packages = ["service"]
dockerfile = "Dockerfile"

[docker.modules.core.service.build_args]
FOO = 1
"#;
    let issues = validate(toml).expect("parses");
    assert!(issues.iter().any(|i| {
        i.path == "docker.modules.core.service.build_args.FOO"
            && i.message.contains("expected a string")
    }));
}

#[test]
fn a_release_commit_message_template_is_a_recognised_field() {
    let toml = "[release]\ncommit_message_template = \"release: {summary}\"\n";
    assert_eq!(validate(toml).expect("parses"), Vec::new());
}

#[test]
fn an_empty_release_commit_message_template_is_reported() {
    let toml = "[release]\ncommit_message_template = \"\"\n";
    let issues = validate(toml).expect("parses");
    assert!(
        issues
            .iter()
            .any(|i| i.path == "release.commit_message_template"
                && i.message.contains("must not be empty"))
    );
}

#[test]
fn several_simultaneous_problems_are_all_reported_at_once() {
    let toml = r#"
bogus_top_level = 1

[docker]
bogus_docker_field = 1

[docker.modules.core]
packages = []
dockerfile = ""

[docker.modules.core.unrelated-package]
bogus_override_field = 1
"#;
    let issues = validate(toml).expect("parses");

    // The point of this command: every one of these is independently
    // detectable, and a single run finds all of them, not just the first.
    assert!(issues.iter().any(|i| i.path == "bogus_top_level"));
    assert!(issues.iter().any(|i| i.path == "docker.bogus_docker_field"));
    assert!(
        issues
            .iter()
            .any(|i| i.path == "docker.modules.core.packages")
    );
    assert!(
        issues
            .iter()
            .any(|i| i.path == "docker.modules.core.dockerfile")
    );
    assert!(
        issues
            .iter()
            .any(|i| i.path == "docker.modules.core.unrelated-package.bogus_override_field")
    );
    assert!(
        issues
            .iter()
            .any(|i| i.path == "docker.modules.core.unrelated-package"
                && i.message.contains("not in this module's"))
    );
    assert!(issues.len() >= 6, "{issues:?}");
}

#[test]
fn issues_are_sorted_by_path_for_stable_output() {
    let toml = "zeta = 1\nalpha = 1\n";
    let issues = validate(toml).expect("parses");
    let paths: Vec<&str> = issues.iter().map(|i| i.path.as_str()).collect();
    let mut sorted = paths.clone();
    sorted.sort_unstable();
    assert_eq!(paths, sorted);
}

#[test]
fn the_estates_own_anvil_toml_has_no_issues() {
    // Regression guard: this validator must agree the real config is clean,
    // not just that a hand-written fixture is.
    let _guard = stable_cwd_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../.anvil.toml");
    let content = std::fs::read_to_string(path).expect("the estate's .anvil.toml");
    assert_eq!(validate(&content).expect("parses"), Vec::new());
}
