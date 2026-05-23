use riveter::config::RiveterConfig;

#[test]
fn test_default_config() {
    let config = RiveterConfig::default();
    assert!(config.env.current.is_none());
}

#[test]
fn test_parse_config() {
    let toml_str = r#"
        [env]
        current = "prod"
    "#;
    let config: RiveterConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(config.env.current, Some("prod".to_string()));
}
