use songbook_service::routers::api::protected_prefix;

#[quench_test::test]
fn the_api_prefix_is_right_for_a_service_at_the_root_and_under_a_base_path() {
    assert_eq!(protected_prefix("/"), "/api/v1");
    assert_eq!(protected_prefix(""), "/api/v1");
    assert_eq!(protected_prefix("/songbook"), "/songbook/api/v1");
}
