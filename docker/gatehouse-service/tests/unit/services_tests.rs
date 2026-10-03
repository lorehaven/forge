use gatehouse_service::catalog::PermissionCatalog;
use gatehouse_service::services::{enabled_services, feature_enabled, service_url};

/// A catalog with a card for each named service, plus gatehouse itself (no card).
fn catalog(home_services: &[&str]) -> PermissionCatalog {
    let dir = std::env::temp_dir().join(format!("services-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("permissions.toml");
    let mut toml = String::from("[services.gatehouse]\nactions = [\"read-users\"]\n");
    for name in home_services {
        toml.push_str(&format!(
            "[services.{name}]\nlabel = \"{name} label\"\nhome = true\ndescription = \"{name} description\"\nactions = [\"read\"]\n"
        ));
    }
    std::fs::write(&path, toml).unwrap();
    PermissionCatalog::load_from(&path.to_string_lossy()).unwrap()
}

fn env_lock() -> &'static std::sync::Mutex<()> {
    static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(()))
}

#[test]
fn feature_enabled_recognizes_every_truthy_and_falsy_spelling() {
    let _guard = env_lock().lock().unwrap_or_else(|p| p.into_inner());
    for value in ["1", "true", "TRUE", "yes", "on"] {
        unsafe { std::env::set_var("GATEHOUSE_TEST_FEATURE_FLAG", value) };
        assert!(
            feature_enabled("GATEHOUSE_TEST_FEATURE_FLAG", false),
            "{value} should be truthy"
        );
    }
    for value in ["0", "false", "FALSE", "no", "off"] {
        unsafe { std::env::set_var("GATEHOUSE_TEST_FEATURE_FLAG", value) };
        assert!(
            !feature_enabled("GATEHOUSE_TEST_FEATURE_FLAG", true),
            "{value} should be falsy"
        );
    }
    unsafe { std::env::remove_var("GATEHOUSE_TEST_FEATURE_FLAG") };
}

#[test]
fn feature_enabled_falls_back_to_the_default_when_unset_or_unrecognized() {
    let _guard = env_lock().lock().unwrap_or_else(|p| p.into_inner());
    unsafe { std::env::remove_var("GATEHOUSE_TEST_FEATURE_FLAG_2") };
    assert!(feature_enabled("GATEHOUSE_TEST_FEATURE_FLAG_2", true));
    assert!(!feature_enabled("GATEHOUSE_TEST_FEATURE_FLAG_2", false));

    unsafe { std::env::set_var("GATEHOUSE_TEST_FEATURE_FLAG_2", "not-a-boolean") };
    assert!(feature_enabled("GATEHOUSE_TEST_FEATURE_FLAG_2", true));
    unsafe { std::env::remove_var("GATEHOUSE_TEST_FEATURE_FLAG_2") };
}

#[test]
fn service_url_prefers_ui_url_over_the_plain_url_and_trims_trailing_slash() {
    let _guard = env_lock().lock().unwrap_or_else(|p| p.into_inner());
    unsafe { std::env::set_var("SAGE_UI_URL", "https://sage.example.test/") };
    unsafe { std::env::set_var("SAGE_URL", "https://sage-internal.example.test") };
    assert_eq!(
        service_url("SAGE"),
        Some("https://sage.example.test".to_string())
    );
    unsafe { std::env::remove_var("SAGE_UI_URL") };
    unsafe { std::env::remove_var("SAGE_URL") };
}

#[test]
fn service_url_is_none_when_neither_var_is_set() {
    let _guard = env_lock().lock().unwrap_or_else(|p| p.into_inner());
    unsafe { std::env::remove_var("SWITCHBOARD_UI_URL") };
    unsafe { std::env::remove_var("SWITCHBOARD_URL") };
    assert_eq!(service_url("SWITCHBOARD"), None);
}

#[test]
fn enabled_services_excludes_a_service_disabled_by_its_feature_flag() {
    let _guard = env_lock().lock().unwrap_or_else(|p| p.into_inner());
    unsafe { std::env::set_var("WAREHOUSE_UI_URL", "https://warehouse.example.test") };
    unsafe { std::env::set_var("FEATURE_WAREHOUSE_ENABLED", "false") };
    unsafe { std::env::remove_var("CONVEYOR_UI_URL") };
    unsafe { std::env::remove_var("CONVEYOR_URL") };
    unsafe { std::env::remove_var("SAGE_UI_URL") };
    unsafe { std::env::remove_var("SAGE_URL") };
    unsafe { std::env::remove_var("SWITCHBOARD_UI_URL") };
    unsafe { std::env::remove_var("SWITCHBOARD_URL") };

    let services = enabled_services(&catalog(&[
        "conveyor",
        "sage",
        "switchboard",
        "warehouse",
        "workbench",
    ]));
    assert!(
        services
            .iter()
            .all(|s| s.card_class != "home-card-warehouse")
    );

    unsafe { std::env::remove_var("WAREHOUSE_UI_URL") };
    unsafe { std::env::remove_var("FEATURE_WAREHOUSE_ENABLED") };
}

#[test]
fn enabled_services_includes_a_configured_and_enabled_service() {
    let _guard = env_lock().lock().unwrap_or_else(|p| p.into_inner());
    unsafe { std::env::set_var("CONVEYOR_UI_URL", "https://conveyor.example.test") };
    unsafe { std::env::remove_var("FEATURE_CONVEYOR_ENABLED") };
    unsafe { std::env::remove_var("SAGE_UI_URL") };
    unsafe { std::env::remove_var("SAGE_URL") };
    unsafe { std::env::remove_var("SWITCHBOARD_UI_URL") };
    unsafe { std::env::remove_var("SWITCHBOARD_URL") };
    unsafe { std::env::remove_var("WAREHOUSE_UI_URL") };
    unsafe { std::env::remove_var("WAREHOUSE_URL") };
    unsafe { std::env::remove_var("WORKBENCH_UI_URL") };
    unsafe { std::env::remove_var("WORKBENCH_URL") };

    let services = enabled_services(&catalog(&[
        "conveyor",
        "sage",
        "switchboard",
        "warehouse",
        "workbench",
    ]));
    assert_eq!(services.len(), 1);
    assert_eq!(services[0].url, "https://conveyor.example.test");
    assert_eq!(services[0].title_key, "ui_service_conveyor_title");

    unsafe { std::env::remove_var("CONVEYOR_UI_URL") };
}

#[test]
fn a_service_added_only_to_the_catalog_gets_a_card_with_derived_keys() {
    let _guard = env_lock().lock().unwrap_or_else(|p| p.into_inner());
    unsafe { std::env::set_var("ARCHIVE_VAULT_UI_URL", "https://vault.example.test/") };
    unsafe { std::env::remove_var("FEATURE_ARCHIVE_VAULT_ENABLED") };

    let services = enabled_services(&catalog(&["archive-vault", "not-deployed"]));
    assert_eq!(services.len(), 1, "only the one with a URL");
    let card = &services[0];
    assert_eq!(card.url, "https://vault.example.test");
    assert_eq!(card.title_key, "ui_service_archive-vault_title");
    assert_eq!(card.desc_key, "ui_service_archive-vault_desc");
    assert_eq!(card.label, "archive-vault label");
    assert_eq!(
        card.description.as_deref(),
        Some("archive-vault description")
    );
    assert_eq!(card.card_class, "home-card-archive-vault");

    unsafe { std::env::remove_var("ARCHIVE_VAULT_UI_URL") };
}

#[test]
fn the_shipped_catalog_offers_every_estate_service_but_not_gatehouse() {
    let shipped = PermissionCatalog::load_from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/config/permissions.toml"
    ))
    .unwrap();
    let names: Vec<_> = shipped
        .home_services()
        .into_iter()
        .map(|s| s.name)
        .collect();
    assert_eq!(
        names,
        [
            "conveyor",
            "gantry",
            "sage",
            "switchboard",
            "warehouse",
            "workbench"
        ]
    );
}
