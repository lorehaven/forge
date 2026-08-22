use actix_web::body::MessageBody;
use warehouse_service::routers::ui::pages::home::render_home_page;

/// `docker_enabled`/`crates_enabled` read a `LazyLock` fixed for the
/// whole test binary from whichever `FEATURE_*_ENABLED` env vars were
/// set the first time anything touched it (see `routers_mod_tests`'s own
/// tests), so this can't control which branch runs - it just asserts
/// the page renders successfully and always carries the title, which
/// holds regardless of which branch that turns out to be.
#[test]
fn render_home_page_always_renders_ok_with_the_home_title() {
    let resp = render_home_page();
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
    let body = resp.into_body().try_into_bytes().unwrap();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("ui_home_title"));
    // Exactly one of "no services" or a service section should show,
    // never both and never neither.
    let has_empty_state = html.contains("ui_home_no_services");
    let has_services_group = html.contains("ui_home_group_services");
    assert_ne!(has_empty_state, has_services_group);
}
