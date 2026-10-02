use rivet_package::path::validate;

#[test]
fn accepts_plain_relative_paths() {
    for ok in [
        "overlay.yaml",
        "templates/web/deployment.yaml.j2",
        "a/b/c.txt",
    ] {
        assert!(validate(ok).is_ok(), "{ok}");
    }
}

#[test]
fn rejects_anything_that_could_escape_or_confuse() {
    for bad in [
        "",
        "/etc/passwd",
        "../x",
        "a/../../x",
        "a/./b",
        "a//b",
        "a/",
        "a\\b",
        "a\0b",
        "a\nb",
    ] {
        assert!(validate(bad).is_err(), "{bad:?}");
    }
}

#[test]
fn rejects_overlong_paths_and_components() {
    assert!(validate(&"a".repeat(256)).is_err());
    assert!(validate(&format!("{}/x", "a/".repeat(300))).is_err());
}
