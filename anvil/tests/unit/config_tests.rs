use anvil::config::Config;

#[test]
fn test_default_config() {
    let config = Config::default();
    assert!(config.modules.is_empty());
    assert!(config.skipped.is_none());
}

#[test]
fn test_parse_config() {
    let toml_str = r#"
        [modules.core]
        packages = ["ferrous"]
        dockerfile = "Dockerfile"

        [skipped]
        modules = ["old"]
    "#;
    let config: Config = toml::from_str(toml_str).unwrap();
    assert_eq!(config.modules.len(), 1);
    assert!(config.modules.contains_key("core"));
    assert_eq!(
        config.modules.get("core").unwrap().packages,
        vec!["ferrous"]
    );
    assert_eq!(config.skipped.unwrap().modules, vec!["old"]);
}
