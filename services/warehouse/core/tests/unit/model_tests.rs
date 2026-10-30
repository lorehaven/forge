use quench_serde::json;
use warehouse_core::{Browse, CrateDep, DockerTag, NewStorage, Overview, RivetEntry, Storages};

#[quench_test::test]
fn the_overview_reads_back() {
    let text = r#"{"username":"dev","can_manage":true,"features":{"docker":true,"crates":false,"files":true,"artifacts":false,"rivets":true}}"#;
    let overview: Overview = json::from_str(text).unwrap();
    assert!(overview.can_manage && overview.features.docker && overview.features.rivets);
    assert!(!overview.features.crates);
    assert_eq!(
        json::from_str::<Overview>(&json::to_string(&overview)).unwrap(),
        overview
    );
}

#[quench_test::test]
fn missing_optional_fields_are_none() {
    let tag: DockerTag = json::from_str(r#"{"tag":"1","digest":"sha256:a"}"#).unwrap();
    assert_eq!((tag.media_type, tag.size_bytes), (None, None));
    let rivet: RivetEntry = json::from_str(
        r#"{"name":"n","version":"1.0.0","size_bytes":3,"sha256":"x","uploaded_by":"u","yanked":false,"published":"p"}"#,
    )
    .unwrap();
    assert!(rivet.meta.is_empty() && rivet.requires_riveter.is_none());
}

#[quench_test::test]
fn a_dependency_line_is_written_as_the_old_page_wrote_it() {
    let dep = CrateDep {
        name: "serde".into(),
        req: "^1".into(),
        optional: true,
        kind: "normal".into(),
        target: Some("cfg(unix)".into()),
        package: None,
    };
    assert_eq!(dep.line(), "serde ^1 [optional] [target: cfg(unix)]");
    let renamed = CrateDep {
        package: Some("real".into()),
        optional: false,
        target: None,
        ..dep
    };
    assert_eq!(renamed.line(), "real (as serde) ^1");
}

#[quench_test::test]
fn bodies_are_the_json_the_service_reads() {
    let body = NewStorage {
        name: "s".into(),
        owner: "o".into(),
        quota_bytes: None,
        max_file_bytes: Some(7),
        sync_enabled: true,
    };
    let text = json::to_string(&body);
    assert!(text.contains(r#""max_file_bytes":7"#), "{text}");
    assert_eq!(json::from_str::<NewStorage>(&text).unwrap(), body);
    let storages: Storages =
        json::from_str(r#"{"fixed":[{"name":"a","root":"/r"}],"dynamic":[]}"#).unwrap();
    assert_eq!(storages.fixed_named("a").unwrap().root, "/r");
    assert!(storages.dynamic_named("a").is_none());
    assert_eq!(Browse::DENIED, "denied");
}
