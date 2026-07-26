//! Unit tests for `actix/routers/ui/pages/auth.rs`.

use quench_auth::actix::routers::ui::pages::auth::*;

/// A login page that redirects anywhere is a phishing primitive, so
/// off-realm targets are dropped rather than followed.
#[test]
fn only_rooted_paths_and_allowed_hosts_are_followed() {
    envmnt::remove("AUTH_REDIRECT_HOSTS");

    assert_eq!(validated_redirect("/ui/home"), Some("/ui/home".to_string()));
    assert_eq!(validated_redirect("https://evil.example.com/"), None);
    // Protocol-relative URLs look like paths but navigate off-origin.
    assert_eq!(validated_redirect("//evil.example.com"), None);
    assert_eq!(validated_redirect("/\\evil.example.com"), None);

    envmnt::set("AUTH_REDIRECT_HOSTS", "https://sage.example.com");
    assert_eq!(
        validated_redirect("https://sage.example.com/ui/home"),
        Some("https://sage.example.com/ui/home".to_string())
    );
    assert_eq!(validated_redirect("https://evil.example.com/"), None);
    envmnt::remove("AUTH_REDIRECT_HOSTS");
}
