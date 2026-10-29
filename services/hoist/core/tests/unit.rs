use hoist_core::{
    Artifact, InstallState, Installed, SELF_NAME, SELF_PROGRAM, announcement_key, build_entries,
    format_bytes, pending_updates, state_for, step, version_code,
};
use quench_serde::json;
use std::collections::BTreeMap;

/// A row as warehouse returns it (`ArtifactView`), captured from the registry's shape.
const ROW: &str = r#"{
  "program": "dev.lorehaven.mathom", "platform": "android", "arch": null, "format": "apk",
  "version_code": 7, "version_name": "0.1.0+7", "filename": "dev.lorehaven.mathom-7.apk",
  "size_bytes": 41758291, "sha256": "9f2c0000000000000000000000000000000000000000000000000000000000aa",
  "label": "Mathom", "min_sdk_version": 24, "target_sdk_version": 35,
  "permissions": ["android.permission.INTERNET"], "uploaded_by": "losseheil",
  "yanked": false, "created_at": "2026-08-26T14:54:15Z"
}"#;

fn artifact(program: &str, label: Option<&str>, code: i64) -> Artifact {
    let mut a: Artifact = json::from_str(ROW).unwrap();
    a.program = program.to_string();
    a.label = label.map(str::to_string);
    a.version_code = code;
    a
}

fn have(program: &str, code: i64) -> (String, Installed) {
    (
        program.to_string(),
        Installed {
            program: program.to_string(),
            version_code: code,
            version_name: String::new(),
        },
    )
}

#[quench_test::test]
fn a_registry_row_is_read_as_it_comes() {
    let a: Artifact = json::from_str(ROW).unwrap();
    assert_eq!(
        (a.program.as_str(), a.platform.as_str(), a.format.as_str()),
        ("dev.lorehaven.mathom", "android", "apk")
    );
    assert_eq!(
        (a.version_code, a.version_name.as_str(), a.size_bytes),
        (7, "0.1.0+7", 41_758_291)
    );
    assert_eq!(
        (a.min_sdk_version, a.target_sdk_version, a.arch.clone()),
        (Some(24), Some(35), None)
    );
    assert_eq!(a.permissions, ["android.permission.INTERNET"]);
    assert_eq!(a.title(), "Mathom");
    assert!(!a.yanked && a.sha256.len() == 64);
}

#[quench_test::test]
fn missing_optional_fields_are_none_not_errors() {
    let minimal = r#"{"program":"app","platform":"linux","format":"deb","version_code":1,"version_name":"1",
        "filename":"app.deb","size_bytes":10,"sha256":"00","uploaded_by":"u","created_at":"2026-01-01T00:00:00Z"}"#;
    let a: Artifact = json::from_str(minimal).unwrap();
    assert_eq!(
        (a.label.clone(), a.arch.clone(), a.min_sdk_version, a.yanked),
        (None, None, None, false)
    );
    assert!(a.permissions.is_empty());
    assert_eq!(a.title(), "app", "no label: the program names it");
    let mut blank = a;
    blank.label = Some(String::new());
    assert_eq!(blank.title(), "app", "an empty label is no label");
    assert!(
        json::from_str::<Artifact>(r#"{"program":"x"}"#).is_err(),
        "a row with no version is not a row"
    );
}

#[quench_test::test]
fn the_state_is_decided_by_version_codes_alone() {
    assert_eq!(state_for(None, 5), InstallState::NotInstalled);
    assert_eq!(state_for(Some(4), 5), InstallState::UpdateAvailable);
    assert_eq!(state_for(Some(5), 5), InstallState::UpToDate);
    assert_eq!(
        state_for(Some(6), 5),
        InstallState::UpToDate,
        "a newer build than the registry's is not an update"
    );
    assert_eq!(state_for(Some(0), 1), InstallState::UpdateAvailable);
}

#[quench_test::test]
fn entries_are_ordered_updates_then_current_then_not_installed() {
    let catalog = vec![
        artifact("c.new", Some("Zeta new"), 1),
        artifact("a.old", Some("beta"), 3),
        artifact("b.cur", Some("Alpha"), 2),
        artifact("d.old", Some("Alpha"), 9),
        artifact("e.new", None, 1),
        artifact("f.cur", Some("charlie"), 4),
    ];
    let installed: BTreeMap<_, _> = [
        have("a.old", 2),
        have("b.cur", 2),
        have("d.old", 8),
        have("f.cur", 4),
    ]
    .into();
    let order: Vec<_> = build_entries(&catalog, &installed, "none")
        .iter()
        .map(|e| (e.latest.program.clone(), e.state()))
        .collect();
    assert_eq!(
        order,
        [
            ("d.old".to_string(), InstallState::UpdateAvailable), // "Alpha" before "beta" (case-blind)
            ("a.old".to_string(), InstallState::UpdateAvailable),
            ("b.cur".to_string(), InstallState::UpToDate),
            ("f.cur".to_string(), InstallState::UpToDate),
            ("e.new".to_string(), InstallState::NotInstalled), // no label: titled by program, "e.new" < "Zeta new"
            ("c.new".to_string(), InstallState::NotInstalled),
        ]
    );
}

#[quench_test::test]
fn the_running_app_is_marked_and_an_installed_program_outside_the_catalog_is_not_listed() {
    let catalog = vec![
        artifact("dev.lorehaven.hoist", Some("Hoist"), 3),
        artifact("other", None, 1),
    ];
    let installed: BTreeMap<_, _> =
        [have("dev.lorehaven.hoist", 2), have("unrelated.app", 9)].into();
    let entries = build_entries(&catalog, &installed, "dev.lorehaven.hoist");
    assert_eq!(
        entries.len(),
        2,
        "unrelated.app is not in the registry, so it is not listed"
    );
    let me = entries.iter().find(|e| e.is_self).unwrap();
    assert_eq!(
        (me.latest.program.as_str(), me.state()),
        ("dev.lorehaven.hoist", InstallState::UpdateAvailable)
    );
    assert!(entries.iter().filter(|e| e.is_self).count() == 1);
}

#[quench_test::test]
fn one_row_per_program_the_highest_that_is_not_yanked() {
    let mut yanked = artifact("app", None, 9);
    yanked.yanked = true;
    let catalog = vec![
        artifact("app", None, 4),
        yanked,
        artifact("app", None, 6),
        artifact("app", None, 5),
    ];
    let entries = build_entries(&catalog, &BTreeMap::new(), "x");
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0].latest.version_code, 6,
        "9 is yanked, so 6 is the latest"
    );
    assert!(build_entries(&[], &BTreeMap::new(), "x").is_empty());
    let all_yanked = {
        let mut a = artifact("gone", None, 1);
        a.yanked = true;
        vec![a]
    };
    assert!(
        build_entries(&all_yanked, &BTreeMap::new(), "x").is_empty(),
        "nothing offerable, nothing listed"
    );
}

#[quench_test::test]
fn sizes_are_shown_in_binary_units() {
    assert_eq!(format_bytes(0), "0 B");
    assert_eq!(format_bytes(-5), "0 B");
    assert_eq!(format_bytes(1023), "1023 B");
    assert_eq!(format_bytes(1024), "1.0 KiB");
    assert_eq!(format_bytes(41_758_291), "39.8 MiB");
    assert_eq!(format_bytes(5 * 1024 * 1024 * 1024), "5.0 GiB");
    assert_eq!(format_bytes(i64::MAX), "8388608.0 TiB");
}

#[quench_test::test]
fn the_scaffolds_counter_still_steps() {
    assert_eq!(step(0, 1), 1);
    assert_eq!(step(i64::MAX, 1), i64::MAX);
}

quench_test::main!();

#[quench_test::test]
fn this_app_is_named_even_when_the_registry_sent_no_label_and_is_installed_at_its_own_version() {
    // Builds published before the registry was told the name have no label: the program id is not what to show.
    let unnamed = artifact(SELF_PROGRAM, None, 1002);
    let other = artifact("dev.example.other", None, 5);
    let mut installed = BTreeMap::new();
    installed.insert(
        SELF_PROGRAM.to_string(),
        Installed {
            program: SELF_PROGRAM.to_string(),
            version_code: version_code("0.1.3"),
            version_name: "0.1.3".to_string(),
        },
    );
    let entries = build_entries(&[unnamed, other], &installed, SELF_PROGRAM);
    let me = entries.iter().find(|e| e.is_self).unwrap();
    assert_eq!(me.latest.title(), SELF_NAME);
    // The running 0.1.3 is newer than the registry's 0.1.2: nothing to install.
    assert_eq!(me.state(), InstallState::UpToDate);
    // Another program with no label keeps its id, and an app that was told its name keeps that name.
    let other = entries.iter().find(|e| !e.is_self).unwrap();
    assert_eq!(other.latest.title(), "dev.example.other");
    assert_eq!(other.state(), InstallState::NotInstalled);
}

#[quench_test::test]
fn a_version_is_one_number_the_same_way_quench_and_anvil_make_it() {
    for (version, code) in [
        ("0.1.0", 1000),
        ("0.1.3", 1003),
        ("1.2.3", 1_002_003),
        ("0.1.3-rc1", 1003),
        ("0.1.3+b7", 1003),
        ("0.1.2000", 1999),
        ("x", 0),
    ] {
        assert_eq!(version_code(version), code, "{version}");
    }
}

#[quench_test::test]
fn the_updates_are_what_is_older_than_the_registry_and_the_same_set_is_announced_once() {
    let mut installed = BTreeMap::new();
    for (program, code) in [
        ("dev.example.a", 1),
        ("dev.example.b", 9),
        ("dev.example.c", 3),
    ] {
        installed.insert(
            program.to_string(),
            Installed {
                program: program.to_string(),
                version_code: code,
                version_name: format!("{code}.0"),
            },
        );
    }
    // a: older than the registry's 2; b: newer than its 5; c: current; d: not installed (not an update).
    let rows = [
        artifact("dev.example.a", Some("Zed"), 2),
        artifact("dev.example.b", Some("Bee"), 5),
        artifact("dev.example.c", Some("Cee"), 3),
        artifact("dev.example.d", Some("Dee"), 4),
    ];
    let entries = build_entries(&rows, &installed, SELF_PROGRAM);
    let updates = pending_updates(&entries);
    assert_eq!(
        updates.iter().map(|u| u.title.as_str()).collect::<Vec<_>>(),
        ["Zed"],
        "only what is installed and older"
    );
    let key = announcement_key(&updates);
    assert_eq!(key, "dev.example.a@2");
    // The same set again is the same key; a newer build of it, or one more program, is not.
    assert_eq!(announcement_key(&pending_updates(&entries)), key);
    installed.insert(
        "dev.example.d".into(),
        Installed {
            program: "dev.example.d".into(),
            version_code: 1,
            version_name: "1.0".into(),
        },
    );
    let more = pending_updates(&build_entries(&rows, &installed, SELF_PROGRAM));
    assert_ne!(announcement_key(&more), key);
    assert_eq!(
        announcement_key(&[]),
        "",
        "nothing to announce is the empty key"
    );
}
