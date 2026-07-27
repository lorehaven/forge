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

#[test]
fn test_build_args_default_to_empty() {
    // Most packages say nothing here: the Dockerfile's own defaults describe a
    // plain web service, and only a package that is not one has anything to add.
    let toml_str = r#"
[docker.modules.core]
packages = ["service"]
dockerfile = "Dockerfile.core"

[docker.modules.core.service]
image_name = "svc"
    "#;
    let config: Config = toml::from_str(toml_str).unwrap();
    let package_override = config
        .docker
        .modules
        .get("core")
        .unwrap()
        .package_overrides
        .get("service")
        .unwrap();

    assert!(package_override.build_args.is_empty());
    // And it still falls back to the module's Dockerfile.
    assert_eq!(package_override.dockerfile, None);
}

#[test]
fn test_parse_build_args() {
    let toml_str = r#"
[docker.modules.docker]
packages = ["foundry-service"]
dockerfile = "docker/Dockerfile.alpine"

[docker.modules.docker.foundry-service]
image_name = "foundry"
build_args = { RESOURCE_DIR = "migrations", RUN_AS = "999:999" }
    "#;
    let config: Config = toml::from_str(toml_str).unwrap();
    let package_override = config
        .docker
        .modules
        .get("docker")
        .unwrap()
        .package_overrides
        .get("foundry-service")
        .unwrap();

    assert_eq!(
        package_override.build_args.get("RESOURCE_DIR").unwrap(),
        "migrations"
    );
    assert_eq!(
        package_override.build_args.get("RUN_AS").unwrap(),
        "999:999"
    );
}

#[test]
fn test_build_args_keep_their_order() {
    // Ordering is why this is a BTreeMap: arguments that reach `docker build` in
    // a different order each run are a build cache that misses at random.
    let toml_str = r#"
[docker.modules.core]
packages = ["service"]
dockerfile = "Dockerfile.core"

[docker.modules.core.service]
build_args = { ZED = "3", ALPHA = "1", MID = "2" }
    "#;
    let config: Config = toml::from_str(toml_str).unwrap();
    let names: Vec<&str> = config.docker.modules["core"].package_overrides["service"]
        .build_args
        .keys()
        .map(String::as_str)
        .collect();

    assert_eq!(names, vec!["ALPHA", "MID", "ZED"]);
}

#[test]
fn test_the_estates_own_config_needs_only_one_dockerfile() {
    // The point of build args: every package under the `docker` module builds
    // from the same template, and the ones that differ say how rather than
    // forking it. A package that reintroduces a `dockerfile` override has to be
    // a deliberate act, not a drift.
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../.anvil.toml");
    let content = std::fs::read_to_string(path).expect("the estate's .anvil.toml");
    let config: Config = toml::from_str(&content).expect("it parses");

    let module = config.docker.modules.get("docker").expect("docker module");
    assert_eq!(module.dockerfile, "docker/Dockerfile.alpine");

    // A package may still name a genuinely different base - switchboard needs a
    // ROCm one for the GPU runtime. What it may not do is name a *variant* of
    // the alpine template, which is the duplication this replaced.
    for package in &module.packages {
        let Some(dockerfile) = module
            .package_overrides
            .get(package)
            .and_then(|o| o.dockerfile.as_deref())
        else {
            continue;
        };
        assert!(
            !dockerfile.ends_with(".alpine"),
            "{package} forks the alpine template as {dockerfile}"
        );
    }

    assert_eq!(
        module.package_overrides["conveyor-service"]
            .build_args
            .get("RUNTIME_PACKAGES")
            .map(String::as_str),
        Some("git"),
        "conveyor checks out a commit before it can read its pipeline"
    );
    assert_eq!(
        module.package_overrides["foundry-service"]
            .build_args
            .get("RESOURCE_DIR")
            .map(String::as_str),
        Some("migrations"),
        "foundry ships a catalog where a web service ships translations"
    );
}
