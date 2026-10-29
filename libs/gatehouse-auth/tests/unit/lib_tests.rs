use gatehouse_auth::*;

fn credentials() -> Credentials {
    Credentials {
        gatehouse_url: "https://host/gatehouse".to_string(),
        app_token: "lht_secret".to_string(),
    }
}

#[quench_test::test]
fn secret_file_is_named_after_the_tool() {
    assert_eq!(secret_file_name("anvil"), ".anvil.secret.toml");
}

#[quench_test::test]
fn load_is_none_without_a_secret_file() {
    let dir = quench_core::tmp::TempDir::new().unwrap();
    assert_eq!(load("anvil", dir.path()).unwrap(), None);
}

#[quench_test::test]
fn save_then_load_round_trips_and_finds_it_from_a_subdirectory() {
    let dir = quench_core::tmp::TempDir::new().unwrap();
    let path = save("anvil", dir.path(), &credentials()).unwrap();
    assert_eq!(path, dir.path().join(".anvil.secret.toml"));

    let nested = dir.path().join("a/b");
    std::fs::create_dir_all(&nested).unwrap();
    assert_eq!(load("anvil", &nested).unwrap(), Some(credentials()));
    assert_eq!(load("riveter", &nested).unwrap(), None);
}

#[cfg(unix)]
#[quench_test::test]
fn saved_file_is_private() {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = quench_core::tmp::TempDir::new().unwrap();
    let path = save("anvil", dir.path(), &credentials()).unwrap();
    let mode = std::fs::metadata(path).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
}

#[quench_test::test]
fn save_adds_the_file_to_gitignore_once() {
    let dir = quench_core::tmp::TempDir::new().unwrap();
    std::fs::write(dir.path().join(".gitignore"), "target").unwrap();
    save("anvil", dir.path(), &credentials()).unwrap();
    save("anvil", dir.path(), &credentials()).unwrap();
    let ignore = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
    assert_eq!(ignore, "target\n.anvil.secret.toml\n");
}

#[quench_test::test]
fn save_leaves_gitignore_alone_when_a_wildcard_already_covers_it() {
    let dir = quench_core::tmp::TempDir::new().unwrap();
    std::fs::write(dir.path().join(".gitignore"), ".*.secret.toml\n").unwrap();
    save("riveter", dir.path(), &credentials()).unwrap();
    let ignore = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
    assert_eq!(ignore, ".*.secret.toml\n");
}

#[quench_test::test]
fn load_reports_a_malformed_file() {
    let dir = quench_core::tmp::TempDir::new().unwrap();
    std::fs::write(dir.path().join(".anvil.secret.toml"), "not = [valid").unwrap();
    assert!(load("anvil", dir.path()).is_err());
}

#[quench_test::test]
fn bearer_prefixes_the_scheme() {
    assert_eq!(bearer("abc"), "Bearer abc");
}

#[quench_test::test]
fn access_token_reports_an_unreachable_gatehouse() {
    let unreachable = Credentials {
        gatehouse_url: "http://127.0.0.1:1".to_string(),
        app_token: "lht_x".to_string(),
    };
    assert!(access_token(&unreachable).is_err());
}

#[quench_test::test]
fn a_tool_with_a_directory_keeps_its_secret_inside_it() {
    let dir = quench_core::tmp::TempDir::new().unwrap();
    std::fs::create_dir(dir.path().join(".anvil")).unwrap();
    std::fs::write(dir.path().join(".gitignore"), "target\n").unwrap();
    let path = save("anvil", dir.path(), &credentials()).unwrap();
    assert_eq!(path, dir.path().join(".anvil/secret.toml"));
    assert!(!dir.path().join(".anvil.secret.toml").exists());
    assert_eq!(
        std::fs::read_to_string(dir.path().join(".gitignore")).unwrap(),
        "target\n.anvil/secret.toml\n"
    );
    let nested = dir.path().join("a/b");
    std::fs::create_dir_all(&nested).unwrap();
    assert_eq!(load("anvil", &nested).unwrap(), Some(credentials()));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[quench_test::test]
fn the_secret_in_the_directory_wins_and_a_new_login_leaves_one_token_behind() {
    let dir = quench_core::tmp::TempDir::new().unwrap();
    // An old secret beside the project's files; then the tool gets a directory.
    save(
        "anvil",
        dir.path(),
        &Credentials {
            gatehouse_url: "https://old/gatehouse".into(),
            app_token: "old".into(),
        },
    )
    .unwrap();
    std::fs::create_dir(dir.path().join(".anvil")).unwrap();
    assert_eq!(
        load("anvil", dir.path()).unwrap().unwrap().app_token,
        "old",
        "still found"
    );
    std::fs::write(
        dir.path().join(".anvil/secret.toml"),
        "gatehouse_url = \"https://new/gatehouse\"\napp_token = \"new\"\n",
    )
    .unwrap();
    assert_eq!(
        load("anvil", dir.path()).unwrap().unwrap().app_token,
        "new",
        "the directory's wins"
    );
    // The next login writes into the directory and takes the old file away.
    let path = save("anvil", dir.path(), &credentials()).unwrap();
    assert_eq!(path, dir.path().join(".anvil/secret.toml"));
    assert!(!dir.path().join(".anvil.secret.toml").exists());
    assert_eq!(load("anvil", dir.path()).unwrap(), Some(credentials()));
}

#[quench_test::test]
fn a_gitignore_that_ignores_every_secret_directory_file_is_enough() {
    let dir = quench_core::tmp::TempDir::new().unwrap();
    std::fs::create_dir(dir.path().join(".anvil")).unwrap();
    std::fs::write(dir.path().join(".gitignore"), ".*/secret.toml\n").unwrap();
    save("anvil", dir.path(), &credentials()).unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.path().join(".gitignore")).unwrap(),
        ".*/secret.toml\n"
    );
}
