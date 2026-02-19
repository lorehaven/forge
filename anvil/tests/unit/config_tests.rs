use anvil::config::Config;

#[test]
fn test_default_config() {
    let config = Config::default();
    assert!(config.docker.modules.is_empty());
    assert!(config.install.packages.is_empty());
}

#[test]
fn test_parse_config() {
    let toml_str = r#"
[docker.modules.core]
packages = ["service"]
dockerfile = "Dockerfile.core"

[install]
packages = ["cli", "service"]
    "#;
    let config: Config = toml::from_str(toml_str).unwrap();
    assert_eq!(config.docker.modules.len(), 1);
    assert!(config.docker.modules.contains_key("core"));
    assert_eq!(
        config.docker.modules.get("core").unwrap().packages,
        vec!["service"]
    );
    assert_eq!(config.install.packages.len(), 2);
}
