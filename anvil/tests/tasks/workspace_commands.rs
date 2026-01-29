use anvil::commands::workspace;

#[test]
#[ignore]
fn list_packages() {
    let _ = workspace::list("names");
}

#[test]
#[ignore]
fn list_packages_json() {
    let _ = workspace::list("json");
}

#[test]
#[ignore]
fn upgrade_dependencies() {
    let _ = workspace::upgrade(false);
}

#[test]
#[ignore]
fn upgrade_dependencies_incompatible() {
    let _ = workspace::upgrade(true);
}

#[test]
#[ignore]
fn audit_dependencies() {
    let _ = workspace::audit();
}

#[test]
#[ignore]
fn machete_unused_dependencies() {
    let _ = workspace::machete();
}
