use anvil::commands::build;

#[test]
#[ignore]
fn build_workspace() {
    let _ = build::build(true, false, false, None);
}

#[test]
#[ignore]
fn build_workspace_all_features() {
    let _ = build::build(true, true, false, None);
}

#[test]
#[ignore]
fn build_workspace_release() {
    let _ = build::build(true, true, true, None);
}

#[test]
#[ignore]
fn build_package_rslibs() {
    let _ = build::build(false, true, false, Some("rslibs".to_string()));
}

#[test]
#[ignore]
fn run_workspace_tests() {
    let _ = build::test(true, None, None, false, false);
}

#[test]
#[ignore]
fn run_tests_rslibs() {
    let _ = build::test(false, Some("rslibs".to_string()), None, false, false);
}
