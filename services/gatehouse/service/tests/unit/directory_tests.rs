use gatehouse_service::directory::parse_listing;

#[quench_test::test]
fn warehouse_lists_only_storages_with_an_owner_by_name() {
    let body = r#"[{"name":"media"},{"name":"photos","owner":"ola","quota_bytes":1},{"name":"backup","owner":"admin"}]"#;
    let found = parse_listing("warehouse", body).unwrap();
    let ids: Vec<&str> = found.iter().map(|c| c.id.as_str()).collect();
    // The fixed storage has no owner and cannot be granted one by one; the rest are sorted by what is read.
    assert_eq!(ids, ["backup", "photos"]);
    assert_eq!(found[1].label, "photos (ola)");
}

#[quench_test::test]
fn workbench_projects_are_chosen_by_id_and_read_by_key_and_name() {
    let body =
        r#"[{"id":"p-2","key":"WB","name":"Workbench"},{"id":"p-1","key":"AB","name":"Alpha"}]"#;
    let found = parse_listing("workbench", body).unwrap();
    assert_eq!(found[0].id, "p-1");
    assert_eq!(found[0].label, "AB · Alpha");
    assert_eq!(found[1].id, "p-2");
}

#[quench_test::test]
fn conveyor_projects_and_gantry_targets_are_listed() {
    let conveyor = parse_listing(
        "conveyor",
        r#"[{"id":"c1","name":"lorehaven","parent_id":null}]"#,
    )
    .unwrap();
    assert_eq!(
        (conveyor[0].id.as_str(), conveyor[0].label.as_str()),
        ("c1", "lorehaven")
    );
    let gantry = parse_listing("gantry", r#"[{"name":"media","namespace":"media"}]"#).unwrap();
    assert_eq!(
        (gantry[0].id.as_str(), gantry[0].label.as_str()),
        ("media", "media")
    );
}

#[quench_test::test]
fn an_entry_without_an_id_is_left_out_and_a_bad_body_says_so() {
    let found = parse_listing(
        "workbench",
        r#"[{"name":"no id"},{"id":"","name":"empty"}]"#,
    )
    .unwrap();
    assert_eq!(found, []);
    assert!(
        parse_listing("workbench", "<html>")
            .unwrap_err()
            .contains("unreadable")
    );
    assert!(
        parse_listing("sage", "[]")
            .unwrap_err()
            .contains("no listing")
    );
}
