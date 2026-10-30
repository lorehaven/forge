use gantry_core::format::{encode, when};
use gantry_core::{DeploymentView, Group, Operation, ResourceRef, StoredPlan};
use quench_serde::json;

const GROUP: &str = r#"{"package":"ml","description":"models","installed":"1.0.0","offered":"1.1.0",
    "status":"update_available","inventoried":true,
    "rows":[
      {"apiVersion":"apps/v1","kind":"Deployment","name":"sage","namespace":"ml","state":"synced","ready":"1/1","workload":true,"editable":false},
      {"apiVersion":"v1","kind":"ConfigMap","name":"sage-config","namespace":"ml","state":"edited","ready":null,"workload":false,"editable":true},
      {"apiVersion":"apps/v1","kind":"Deployment","name":"trainer","namespace":"ml","state":"missing","ready":null,"workload":true,"editable":false}],
    "summary":{"sync":"missing","total":3,"synced":1,"edited":1,"missing":1,"extra":0,"workloads":2,"not_ready":0,"update_to":"1.1.0"},
    "can":["read","scale"],"someday":"an unknown field"}"#;

#[quench_test::test]
fn the_resources_json_reads_back_and_unknown_fields_are_ignored() {
    let group: Group = json::from_str(GROUP).unwrap();
    assert_eq!(group.package, "ml");
    assert_eq!(group.rows.len(), 3);
    assert_eq!(group.rows[0].api_version.as_deref(), Some("apps/v1"));
    assert!(group.may("scale") && !group.may("deploy"));
    assert_eq!(group.versions(), "1.0.0 → 1.1.0");
    assert_eq!(
        json::from_str::<Group>(&json::to_string(&group)).unwrap(),
        group
    );
}

#[quench_test::test]
fn rows_are_filtered_by_kind_state_and_a_piece_of_the_name() {
    let group: Group = json::from_str(GROUP).unwrap();
    assert_eq!(group.kinds(), ["ConfigMap", "Deployment"]);
    assert_eq!(group.filtered("", "", "").len(), 3);
    assert_eq!(group.filtered("deployment", "", "").len(), 2);
    assert_eq!(group.filtered("", "edited", "")[0].name, "sage-config");
    assert_eq!(group.filtered("", "", " TRAIN ")[0].name, "trainer");
    assert!(group.filtered("ConfigMap", "missing", "").is_empty());
}

#[quench_test::test]
fn a_deployment_offers_start_and_stop_by_what_is_up() {
    let view = |observed: &str| DeploymentView {
        name: "inference".into(),
        declared: true,
        target: "ml".into(),
        description: None,
        observed: observed.into(),
        conflicts_with: vec![],
    };
    assert!(view("running").can_stop() && !view("running").can_start());
    assert!(view("partial").can_stop() && view("partial").can_start());
    assert!(!view("stopped").can_stop() && view("stopped").can_start());
    assert!(!view("absent").can_stop() && !view("absent").can_start());
}

#[quench_test::test]
fn an_operation_shows_the_live_log_while_it_runs_and_the_kept_one_after() {
    let text = r#"{"id":"o1","title":"Upgrade ml","state":"running","log":"old","live_log":"now",
        "step_lines":["download ml 1.1.0"],"plan":{"steps":[],"summary":["a"]}}"#;
    let mut op: Operation = json::from_str(text).unwrap();
    assert!(!op.is_terminal());
    assert_eq!(op.log_text(), "now");
    op.state = "succeeded".into();
    assert!(op.is_terminal());
    assert_eq!(op.log_text(), "old");
    op.log = None;
    assert_eq!(op.log_text(), "");
}

#[quench_test::test]
fn a_plan_being_claimed_is_not_yet_an_operation() {
    let mut plan: StoredPlan = json::from_str(
        r#"{"id":"p","target":"ml","action":"start","operation_id":"claiming","plan":{"deployments":["inference"]}}"#,
    )
    .unwrap();
    assert_eq!(plan.confirmed(), None);
    assert!(plan.about_deployments());
    plan.operation_id = Some("o1".into());
    assert_eq!(plan.confirmed(), Some("o1"));
}

#[quench_test::test]
fn a_resource_travels_in_an_address_and_comes_back() {
    let reference = ResourceRef {
        package: "ml".into(),
        api_version: "v1".into(),
        kind: "ConfigMap".into(),
        name: "sage config".into(),
        namespace: String::new(),
    };
    let query = reference.query();
    assert_eq!(
        query,
        "package=ml&kind=ConfigMap&name=sage%20config&api_version=v1"
    );
    let pairs = vec![
        ("package".to_string(), "ml".to_string()),
        ("kind".to_string(), "ConfigMap".to_string()),
        ("name".to_string(), "sage config".to_string()),
        ("api_version".to_string(), "v1".to_string()),
    ];
    assert_eq!(ResourceRef::from_pairs(&pairs), Some(reference.clone()));
    assert_eq!(ResourceRef::from_pairs(&pairs[..2]), None);
    // The body names the API's `apiVersion`, and leaves an empty namespace out.
    let body = json::to_string(&reference.body());
    assert!(
        body.contains(r#""apiVersion":"v1""#) && body.contains(r#""namespace":null"#),
        "{body}"
    );
    assert_eq!(reference.title(), "ConfigMap/sage config");
}

#[quench_test::test]
fn text_for_the_screens() {
    assert_eq!(encode("a b/ü"), "a%20b%2F%C3%BC");
    assert_eq!(when("2026-10-29T10:00:05.123Z"), "2026-10-29 10:00:05");
    assert_eq!(when("2026-10-29T10:00:05Z"), "2026-10-29 10:00:05");
    assert_eq!(when(""), "");
}
