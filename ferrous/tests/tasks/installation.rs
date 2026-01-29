use std::process::Command;

#[test]
#[ignore]
fn install_ferrous() {
    let workspace_root = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");

    let _ = Command::new("cargo")
        .arg("install")
        .arg("--path")
        .arg(&workspace_root)
        .status();
}
