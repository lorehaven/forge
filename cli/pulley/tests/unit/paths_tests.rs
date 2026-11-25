use pulley::paths::{cygdrive_to_drive, drive_to_cygdrive, for_rsync, native};

#[quench_test::test]
fn a_cygdrive_path_has_a_windows_spelling() {
    let drive = |path| cygdrive_to_drive(path);
    assert_eq!(
        drive("/cygdrive/c/Users/me"),
        Some("C:/Users/me".to_string())
    );
    assert_eq!(drive("/cygdrive/D/data/"), Some("D:/data/".to_string()));
    assert_eq!(drive("/cygdrive/c"), Some("C:/".to_string()));
    assert_eq!(drive("/cygdrive/c/"), Some("C:/".to_string()));
}

#[quench_test::test]
fn other_paths_are_not_cygdrive_paths() {
    for path in [
        "/cygdrive",
        "/cygdrive/",
        "/cygdrive/cc/data",
        "/cygdrive/1/data",
        "/c/data",
        "relative/cygdrive/c",
        "host:/cygdrive/c",
    ] {
        assert_eq!(cygdrive_to_drive(path), None, "{path}");
    }
}

#[quench_test::test]
fn a_drive_path_has_a_cygdrive_spelling() {
    let cygdrive = |path| drive_to_cygdrive(path);
    assert_eq!(
        cygdrive(r"C:\Users\me\My Documents"),
        Some("/cygdrive/c/Users/me/My Documents".to_string())
    );
    assert_eq!(cygdrive("d:/data"), Some("/cygdrive/d/data".to_string()));
    assert_eq!(cygdrive(r"E:\"), Some("/cygdrive/e/".to_string()));
    assert_eq!(cygdrive("E:"), Some("/cygdrive/e".to_string()));
}

#[quench_test::test]
fn hosts_and_other_paths_have_no_drive() {
    for path in [
        "c:data",
        "host:/dir",
        "user@c:/dir",
        "rsync://host/module",
        "/cygdrive/c/data",
        r"..\data",
        "",
    ] {
        assert_eq!(drive_to_cygdrive(path), None, "{path}");
    }
}

#[cfg(not(windows))]
#[quench_test::test]
fn paths_are_left_as_they_are_away_from_windows() {
    assert_eq!(native("/cygdrive/c/data"), "/cygdrive/c/data");
    assert_eq!(for_rsync("c:/data"), "c:/data");
}

#[cfg(windows)]
#[quench_test::test]
fn windows_gives_each_side_its_own_spelling() {
    assert_eq!(native("/cygdrive/c/data"), "C:/data");
    assert_eq!(for_rsync(r"C:\data"), "/cygdrive/c/data");
    assert!(!pulley::rsync::is_remote(r"C:\data"));
    assert!(pulley::rsync::is_remote("host:/data"));
}
