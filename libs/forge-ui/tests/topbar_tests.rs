use forge_ui::{css_rules, home_button, user_menu};

#[test]
fn home_button_is_an_icon_link_with_a_screen_reader_label() {
    let html = home_button("/ui/home").render();
    assert!(html.contains("fa-house"));
    assert!(html.contains("href=\"/ui/home\""));
    assert!(html.contains("ui_home_button"));
}

#[test]
fn user_menu_offers_profile_and_logout() {
    let html = user_menu("/ui/account", "/ui/logout").render();
    assert!(html.contains("fa-user"));
    assert!(html.contains("href=\"/ui/account\""));
    assert!(html.contains("ui_profile_edit"));
    assert!(html.contains("href=\"/ui/logout\""));
    assert!(html.contains("ui_logout"));
}

#[test]
fn css_covers_the_menu() {
    let css: String = css_rules().iter().map(|r| r.render()).collect();
    assert!(css.contains("topbar-user-list"));
}

#[test]
fn user_menu_swaps_the_icon_for_the_avatar_once_it_loads() {
    let html = user_menu("/gatehouse/ui/account", "/ui/logout").render();
    assert!(html.contains("src=\"/gatehouse/ui/account/avatar\""));
    assert!(html.contains("has-avatar"));
}
