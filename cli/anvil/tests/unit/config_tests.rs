use anvil::config::Config;

#[test]
fn test_default_config() {
    let config = Config::default();
    assert!(config.docker.modules.is_empty());
    assert!(config.install.packages.is_empty());
    assert!(config.release.registry.is_empty());
    assert!(config.release.packages.is_empty());
}

#[test]
fn test_parse_config() {
    let toml_str = r#"
[docker]
registry = "ghcr.io/acme"

[docker.modules.core]
packages = ["service"]
dockerfile = "Dockerfile.core"

[docker.modules.core.service]
dockerfile = "Dockerfile.service.override"
image_name = "svc-override"
registries = ["registry.internal/override", "backup-registry.internal/override"]

[install]
packages = ["cli", "service"]

[release]
registry = "forge-registry"
packages = ["service"]
    "#;
    let config: Config = toml::from_str(toml_str).unwrap();
    assert_eq!(config.docker.modules.len(), 1);
    assert!(config.docker.modules.contains_key("core"));
    assert_eq!(
        config.docker.modules.get("core").unwrap().packages,
        vec!["service"]
    );
    assert_eq!(config.docker.registry, "ghcr.io/acme");
    let service_override = config
        .docker
        .modules
        .get("core")
        .unwrap()
        .package_overrides
        .get("service")
        .unwrap();
    assert_eq!(
        service_override.dockerfile.as_deref(),
        Some("Dockerfile.service.override")
    );
    assert_eq!(service_override.image_name.as_deref(), Some("svc-override"));
    assert_eq!(
        service_override.registries,
        vec![
            "registry.internal/override".to_string(),
            "backup-registry.internal/override".to_string()
        ]
    );
    assert_eq!(config.install.packages.len(), 2);
    assert_eq!(config.release.registry, "forge-registry");
    assert_eq!(config.release.packages, vec!["service"]);
}

#[test]
fn test_parse_nested_package_overrides_without_global_registry() {
    let toml_str = r#"
[docker.modules.warehouse]
packages = ["warehouse-service"]
dockerfile = "docker/Dockerfile.alpine"

[docker.modules.warehouse.warehouse-service]
dockerfile = "docker/Dockerfile.alpine"
image_name = "warehouse"
registries = ["ossiriand.arda:8080/forge"]
    "#;
    let config: Config = toml::from_str(toml_str).unwrap();
    assert_eq!(config.docker.registry, "");
    let warehouse_module = config.docker.modules.get("warehouse").unwrap();
    assert_eq!(warehouse_module.packages, vec!["warehouse-service"]);
    let package_override = warehouse_module
        .package_overrides
        .get("warehouse-service")
        .unwrap();
    assert_eq!(
        package_override.dockerfile.as_deref(),
        Some("docker/Dockerfile.alpine")
    );
    assert_eq!(package_override.image_name.as_deref(), Some("warehouse"));
    assert_eq!(
        package_override.registries,
        vec!["ossiriand.arda:8080/forge".to_string()]
    );
}
