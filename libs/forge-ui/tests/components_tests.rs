use forge_ui::*;

#[test]
fn user_menu_offers_profile_and_logout() {
    let html = user_menu("/ui/account", "/ui/logout").render();
    assert!(html.contains("href=\"/ui/account\""));
    assert!(html.contains("ui_profile_edit"));
    assert!(html.contains("href=\"/ui/logout\""));
    assert!(html.contains("ui_logout"));
}

#[test]
fn user_menu_swaps_the_icon_for_the_avatar_once_it_loads() {
    let html = user_menu("/gatehouse/ui/account", "/ui/logout").render();
    assert!(html.contains("src=\"/gatehouse/ui/account/avatar\""));
    assert!(html.contains("has-avatar"));
}

#[test]
fn stylesheet_has_tokens_both_themes_and_mobile_rules() {
    let css = stylesheet(&[]);
    assert!(css.contains("--fui-brand-600"));
    assert!(css.contains("html[data-fui-theme=\"light\"]"));
    assert!(css.contains("@media (max-width: 1023.98px)"));
}

#[test]
fn stylesheet_appends_service_rules_last() {
    let css = stylesheet(&[quench_web::prelude::CssRule::new(".svc-x").property("color", "red")]);
    assert!(css.rfind(".svc-x").unwrap() > css.rfind(".fui-btn").unwrap());
}

#[test]
fn button_variants_map_to_classes() {
    let html = button_el(Variant::Danger, "k").render();
    assert!(html.contains("fui-btn-danger"));
    assert!(html.contains("data-i18n=\"k\""));
    assert!(
        submit_button(Variant::Primary, "k")
            .render()
            .contains("type=\"submit\"")
    );
}

#[test]
fn alert_error_has_alert_role() {
    assert!(
        alert(Tone::Error, txt("x"))
            .render()
            .contains("role=\"alert\"")
    );
}

#[test]
fn page_header_renders_crumbs_and_actions() {
    let html = PageHeader::new("title")
        .crumb(txt("Home"), Some("/h"))
        .crumb(txt("Here"), None)
        .action(button_el(Variant::Primary, "go"))
        .build()
        .render();
    assert!(html.contains("href=\"/h\""));
    assert!(html.contains("fui-crumb-current"));
    assert!(html.contains("fui-page-actions"));
}

#[test]
fn modal_wires_open_and_close() {
    assert_eq!(open_modal("m"), "fuiOpenModal('m')");
    let html = modal("m", "t", txt("b"), None).render();
    assert!(html.contains("id=\"m\""));
    assert!(html.contains("fuiCloseModal"));
}

#[test]
fn table_can_opt_into_stacking() {
    assert!(
        table_el(&["a"], vec![], true)
            .render()
            .contains("fui-table-stack")
    );
    assert!(
        !table_el(&["a"], vec![], false)
            .render()
            .contains("fui-table-stack")
    );
}
