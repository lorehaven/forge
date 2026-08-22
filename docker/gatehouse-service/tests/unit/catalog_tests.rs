use gatehouse_service::catalog::PermissionCatalog;

/// A fixture file per call, under a name unique enough that concurrent
/// tests never collide - `load_from` takes an explicit path precisely so
/// this does not have to go through a process-global environment variable.
fn load(toml: &str) -> anyhow::Result<PermissionCatalog> {
    let dir = std::env::temp_dir().join(format!("permcat-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("permissions.toml");
    std::fs::write(&path, toml).unwrap();
    let result = PermissionCatalog::load_from(&path.to_string_lossy());
    let _ = std::fs::remove_dir_all(&dir);
    result
}

#[test]
fn a_service_lists_its_declared_actions() {
    let catalog = load(
        r#"
        [services.sage]
        actions = ["read", "write"]
        "#,
    )
    .unwrap();

    assert_eq!(catalog.actions_for("sage"), ["read", "write"]);
    assert!(catalog.is_known_action("sage", "write"));
    assert!(!catalog.is_known_action("sage", "delete-everything"));
    assert!(!catalog.is_known_service("warehouse"));
}

#[test]
fn a_resource_scoped_action_is_known_when_its_type_and_base_action_are() {
    let catalog = load(
        r#"
        [services.conveyor]
        actions = ["read", "write"]
        resource_types = ["project"]
        "#,
    )
    .unwrap();

    assert!(catalog.is_known_action("conveyor", "project:abc-123:write"));
    assert!(catalog.is_known_action("conveyor", "project:abc-123:read"));
    // The resource id itself is never validated - any string in the
    // middle segment is accepted.
    assert!(catalog.is_known_action("conveyor", "project:does-not-exist:read"));
    // An undeclared resource type, or a base action the service does not
    // grant, is still rejected.
    assert!(!catalog.is_known_action("conveyor", "repo:abc-123:read"));
    assert!(!catalog.is_known_action("conveyor", "project:abc-123:launch"));
}

#[test]
fn a_template_expands_to_a_permissions_map() {
    let catalog = load(
        r#"
        [services.sage]
        actions = ["read", "write"]
        [services.warehouse]
        actions = ["read", "write"]
        [templates.viewer]
        sage = ["read"]
        warehouse = ["read"]
        "#,
    )
    .unwrap();

    let viewer = catalog.template("viewer").unwrap();
    assert_eq!(
        viewer.get("sage").cloned().unwrap_or_default(),
        ["read"].map(str::to_string).into()
    );
    assert!(catalog.template("nonexistent").is_none());
}

#[test]
fn a_template_naming_an_unknown_action_fails_to_load() {
    let err = load(
        r#"
        [services.sage]
        actions = ["read"]
        [templates.editor]
        sage = ["write"]
        "#,
    )
    .unwrap_err();

    assert!(err.to_string().contains("sage:write"));
}

#[test]
fn a_dangling_default_template_fails_to_load() {
    let err = load(
        r#"
        [services.sage]
        actions = ["read"]
        [registration]
        default_template = "nonexistent"
        "#,
    )
    .unwrap_err();

    assert!(err.to_string().contains("nonexistent"));
}

#[test]
fn no_default_template_means_registration_grants_nothing() {
    let catalog = load(
        r#"
        [services.sage]
        actions = ["read"]
        "#,
    )
    .unwrap();

    assert!(catalog.default_registration_grants().is_empty());
}

#[test]
fn an_empty_catalog_fails_to_load() {
    let err = load("").unwrap_err();
    assert!(err.to_string().contains("no [services"));
}
