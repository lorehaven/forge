use anvil::config::{Config, files, load_config, load_from, merge};
use std::path::{Path, PathBuf};

use crate::support;
use support::stable_cwd_lock;

#[quench_test::test]
fn test_default_config() {
    let config = Config::default();
    assert!(config.docker.modules.is_empty());
    assert!(config.install.packages.is_empty());
    assert!(config.release.registry.is_empty());
    assert!(config.release.packages.is_empty());
}

#[quench_test::test]
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
    let config: Config = quench_serde::toml::from_str(toml_str).unwrap();
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

#[quench_test::test]
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
    let config: Config = quench_serde::toml::from_str(toml_str).unwrap();
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

#[quench_test::test]
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
    let config: Config = quench_serde::toml::from_str(toml_str).unwrap();
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

#[quench_test::test]
fn test_parse_build_args() {
    let toml_str = r#"
[docker.modules.docker]
packages = ["foundry-service"]
dockerfile = "docker/Dockerfile.alpine"

[docker.modules.docker.foundry-service]
image_name = "foundry"
build_args = { RESOURCE_DIR = "migrations", RUN_AS = "999:999" }
    "#;
    let config: Config = quench_serde::toml::from_str(toml_str).unwrap();
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

#[quench_test::test]
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
    let config: Config = quench_serde::toml::from_str(toml_str).unwrap();
    let names: Vec<&str> = config.docker.modules["core"].package_overrides["service"]
        .build_args
        .keys()
        .map(String::as_str)
        .collect();

    assert_eq!(names, vec!["ALPHA", "MID", "ZED"]);
}

#[quench_test::test]
fn test_the_estates_own_config_needs_only_one_dockerfile() {
    // The point of build args: every package under the `docker` module builds
    // from the same template, and the ones that differ say how rather than
    // forking it. A package that reintroduces a `dockerfile` override has to be
    // a deliberate act, not a drift.
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let config = load_from(&root).expect("the estate's configuration");

    let module = config.docker.modules.get("docker").expect("docker module");
    assert_eq!(module.dockerfile, "images/Dockerfile.alpine");

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
        module.package_overrides["conveyor"]
            .build_args
            .get("RUNTIME_PACKAGES")
            .map(String::as_str),
        Some("git"),
        "conveyor checks out a commit before it can read its pipeline"
    );
    assert_eq!(
        module.package_overrides["foundry"]
            .build_args
            .get("RESOURCE_DIR")
            .map(String::as_str),
        Some("migrations"),
        "foundry ships a catalog where a web service ships translations"
    );
}

#[quench_test::test]
fn load_config_defaults_when_anvil_toml_is_missing() {
    let _guard = stable_cwd_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let original = std::env::current_dir().unwrap();
    let dir =
        std::env::temp_dir().join(format!("anvil-config-test-missing-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_current_dir(&dir).unwrap();

    let config = load_config(false).expect("falls back to default rather than erroring");
    assert!(config.docker.modules.is_empty());

    std::env::set_current_dir(&original).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[quench_test::test]
fn load_config_defaults_when_anvil_toml_is_invalid_toml() {
    let _guard = stable_cwd_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let original = std::env::current_dir().unwrap();
    let dir =
        std::env::temp_dir().join(format!("anvil-config-test-invalid-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(".anvil.toml"), "not [ valid toml").unwrap();
    std::env::set_current_dir(&dir).unwrap();

    let config = load_config(false).expect("falls back to default rather than erroring");
    assert!(config.docker.modules.is_empty());

    std::env::set_current_dir(&original).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[quench_test::test]
fn load_config_parses_a_valid_anvil_toml() {
    let _guard = stable_cwd_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let original = std::env::current_dir().unwrap();
    let dir = std::env::temp_dir().join(format!("anvil-config-test-valid-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join(".anvil.toml"),
        "[install]\npackages = [\"anvil\"]\n",
    )
    .unwrap();
    std::env::set_current_dir(&dir).unwrap();

    let config = load_config(false).expect("parses");
    assert_eq!(config.install.packages, vec!["anvil".to_string()]);

    std::env::set_current_dir(&original).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

fn project(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("anvil-config-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join(".anvil")).unwrap();
    dir
}

fn put(dir: &Path, name: &str, text: &str) {
    std::fs::write(dir.join(name), text).unwrap();
}

fn names(root: &Path, list: &[PathBuf]) -> Vec<String> {
    list.iter()
        .map(|p| p.strip_prefix(root).unwrap().to_string_lossy().to_string())
        .collect()
}

#[quench_test::test]
fn the_configuration_is_the_single_file_then_the_parts_of_the_directory_by_name_without_the_secret()
{
    let root = project("order");
    put(&root, ".anvil.toml", "[install]\npackages = [\"a\"]\n");
    put(&root, ".anvil/b.toml", "");
    put(&root, ".anvil/a.toml", "");
    put(
        &root,
        ".anvil/secret.toml",
        "gatehouse_url = \"x\"\napp_token = \"t\"\n",
    );
    put(&root, ".anvil/notes.txt", "not toml");
    std::fs::create_dir_all(root.join(".anvil/deeper")).unwrap();
    put(&root, ".anvil/deeper/c.toml", "");
    assert_eq!(
        names(&root, &files(&root)),
        [".anvil.toml", ".anvil/a.toml", ".anvil/b.toml"]
    );
    let _ = std::fs::remove_dir_all(root);
}

#[quench_test::test]
fn tables_in_different_files_become_one_table() {
    let root = project("merge");
    put(
        &root,
        ".anvil/docker.toml",
        "[docker]\nregistry = \"r.example/forge\"\n\n[docker.modules.docker]\ndockerfile = \"d\"\npackages = [\"x-service\"]\n",
    );
    put(
        &root,
        ".anvil/x.toml",
        "[docker.modules.docker.x-service]\nimage_name = \"x\"\n",
    );
    put(
        &root,
        ".anvil/release.toml",
        "[release]\nregistry = \"ennor\"\n",
    );
    put(
        &root,
        ".anvil/hoist.toml",
        "[artifacts.programs.hoist]\nprogram = \"p\"\nversion_file = \"Cargo.toml\"\n",
    );
    put(
        &root,
        ".anvil/artifacts.toml",
        "[artifacts]\nwarehouse_url = \"https://w\"\n",
    );
    let config = load_from(&root).unwrap();
    assert_eq!(config.docker.registry, "r.example/forge");
    let module = &config.docker.modules["docker"];
    assert_eq!(module.packages, ["x-service"]);
    assert_eq!(
        module.package_overrides["x-service"].image_name.as_deref(),
        Some("x")
    );
    assert_eq!(config.release.registry, "ennor");
    assert_eq!(config.artifacts.warehouse_url.as_deref(), Some("https://w"));
    assert_eq!(config.artifacts.programs["hoist"].program, "p");
    let _ = std::fs::remove_dir_all(root);
}

#[quench_test::test]
fn the_single_file_still_works_alone_and_beside_a_directory() {
    let root = project("alone");
    std::fs::remove_dir_all(root.join(".anvil")).unwrap();
    put(&root, ".anvil.toml", "[release]\nregistry = \"one\"\n");
    assert_eq!(load_from(&root).unwrap().release.registry, "one");
    std::fs::create_dir(root.join(".anvil")).unwrap();
    put(
        &root,
        ".anvil/install.toml",
        "[install]\npackages = [\"p\"]\n",
    );
    let config = load_from(&root).unwrap();
    assert_eq!(
        (
            config.release.registry.as_str(),
            config.install.packages.as_slice()
        ),
        ("one", &["p".to_string()][..])
    );
    let _ = std::fs::remove_dir_all(root);
}

#[quench_test::test]
fn the_same_value_in_two_files_is_an_error_that_names_both() {
    let root = project("conflict");
    put(&root, ".anvil.toml", "[release]\nregistry = \"one\"\n");
    put(
        &root,
        ".anvil/release.toml",
        "[release]\nregistry = \"two\"\n",
    );
    let message = format!("{:#}", merge(&root).unwrap_err());
    assert!(
        message.contains("`release.registry`")
            && message.contains(".anvil.toml")
            && message.contains("release.toml"),
        "{message}"
    );
    // Arrays are values too: a list is not spread over files.
    put(&root, ".anvil.toml", "[install]\npackages = [\"a\"]\n");
    put(
        &root,
        ".anvil/release.toml",
        "[install]\npackages = [\"b\"]\n",
    );
    assert!(format!("{:#}", merge(&root).unwrap_err()).contains("`install.packages`"));
    // A table in one file and a value in the other is a clash as well.
    put(&root, ".anvil.toml", "release = 1\n");
    put(
        &root,
        ".anvil/release.toml",
        "[release]\nregistry = \"x\"\n",
    );
    assert!(format!("{:#}", merge(&root).unwrap_err()).contains("`release`"));
    let _ = std::fs::remove_dir_all(root);
}

#[quench_test::test]
fn a_file_that_is_not_toml_is_named() {
    let root = project("broken");
    put(&root, ".anvil/ok.toml", "[install]\npackages = []\n");
    put(&root, ".anvil/bad.toml", "not [ valid");
    let message = format!("{:#}", merge(&root).unwrap_err());
    assert!(
        message.contains("bad.toml") && message.contains("not valid TOML"),
        "{message}"
    );
    let _ = std::fs::remove_dir_all(root);
}

#[quench_test::test]
fn no_configuration_at_all_is_an_error_for_the_strict_loader_and_an_empty_config_for_the_tolerant_one()
 {
    let root = project("none");
    std::fs::remove_dir_all(root.join(".anvil")).unwrap();
    assert!(format!("{:#}", merge(&root).unwrap_err()).contains("no .anvil.toml"));
    let _ = std::fs::remove_dir_all(root);
}
