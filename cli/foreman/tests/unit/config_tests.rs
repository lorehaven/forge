use anyhow::Result;
use foreman::config::*;

fn parse(text: &str) -> Result<Config> {
    toml::from_str::<Config>(text).map_err(anyhow::Error::from)
}

const VALID: &str = r#"
        [project]
        name = "demo"

        [[services]]
        name = "auth"
        package = "auth-svc"
        port = 8080

        [[services]]
        name = "web"
        package = "web-svc"
        port = 8081
        needs = ["auth"]
    "#;

#[test]
fn a_minimal_config_parses_and_validates() {
    let config = parse(VALID).unwrap();
    config.validate().unwrap();
    assert_eq!(config.services.len(), 2);
}

#[test]
fn duplicate_service_names_are_rejected() {
    let text = r#"
            [[services]]
            name = "auth"
            package = "auth-svc"
            port = 8080

            [[services]]
            name = "auth"
            package = "auth-svc-2"
            port = 8081
        "#;
    let err = parse(text).unwrap().validate().unwrap_err().to_string();
    assert!(err.contains("auth"), "error was: {err}");
    assert!(err.contains("more than once"), "error was: {err}");
}

#[test]
fn a_service_needing_an_unknown_service_is_rejected() {
    let text = r#"
            [[services]]
            name = "web"
            package = "web-svc"
            port = 8080
            needs = ["missing"]
        "#;
    let err = parse(text).unwrap().validate().unwrap_err().to_string();
    assert!(err.contains("web"));
    assert!(err.contains("missing"));
}

#[test]
fn a_task_with_an_empty_command_is_rejected() {
    let text = r#"
            [tasks.migrate]
            command = []
        "#;
    let err = parse(text).unwrap().validate().unwrap_err().to_string();
    assert!(err.contains("migrate"));
    assert!(err.contains("empty command"));
}

#[test]
fn a_task_wanting_an_unconfigured_container_is_rejected() {
    let text = r#"
            [tasks.migrate]
            command = ["echo", "hi"]
            containers = ["db"]
        "#;
    let err = parse(text).unwrap().validate().unwrap_err().to_string();
    assert!(err.contains("migrate"));
    assert!(err.contains("db"));
}

#[test]
fn a_task_wanting_a_configured_container_is_accepted() {
    let text = r#"
            [[containers]]
            name = "db"
            image = "postgres:16"

            [tasks.migrate]
            command = ["echo", "hi"]
            containers = ["db"]
        "#;
    parse(text).unwrap().validate().unwrap();
}

#[test]
fn a_note_conditional_on_an_unknown_service_is_rejected() {
    let text = r#"
            [[notes]]
            label = "x"
            message = "y"
            when_selected = "missing"
        "#;
    let err = parse(text).unwrap().validate().unwrap_err().to_string();
    assert!(err.contains("missing"));
}

#[test]
fn an_unknown_top_level_key_is_a_hard_error() {
    let text = r#"
            [project]
            name = "demo"
            typo_field = true
        "#;
    assert!(parse(text).is_err());
}

#[test]
fn service_and_container_lookup_by_name() {
    let config = parse(VALID).unwrap();
    assert!(config.service("auth").is_some());
    assert!(config.service("missing").is_none());
    assert!(config.container("anything").is_none());
}

#[test]
fn tasks_with_role_filters_and_keeps_names() {
    let text = r#"
            [tasks.migrate]
            role = "migrate"
            command = ["echo", "migrate"]

            [tasks.build]
            command = ["echo", "build"]
        "#;
    let config = parse(text).unwrap();
    let migrate = config.tasks_with_role(Role::Migrate);
    assert_eq!(migrate.len(), 1);
    assert_eq!(migrate[0].0, "migrate");

    let manual = config.tasks_with_role(Role::Manual);
    assert_eq!(manual.len(), 1);
    assert_eq!(manual[0].0, "build");
}

#[test]
fn discover_finds_a_config_in_the_starting_directory() {
    let dir = std::env::temp_dir().join(format!("foreman-config-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("foreman.toml"), VALID).unwrap();

    let (root, path) = Config::discover(&dir).unwrap();
    assert_eq!(root, dir);
    assert_eq!(path, dir.join("foreman.toml"));

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn discover_walks_up_to_find_a_config_in_a_parent() {
    let root =
        std::env::temp_dir().join(format!("foreman-config-test-parent-{}", std::process::id()));
    let nested = root.join("a").join("b");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::write(root.join(".foreman.toml"), VALID).unwrap();

    let (found_root, path) = Config::discover(&nested).unwrap();
    assert_eq!(found_root, root);
    assert_eq!(path, root.join(".foreman.toml"));

    std::fs::remove_dir_all(&root).ok();
}
