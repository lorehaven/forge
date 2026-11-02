use songbook_service::routers::api::protected_prefix;

#[quench_test::test]
fn the_api_prefix_is_right_for_a_service_at_the_root_and_under_a_base_path() {
    assert_eq!(protected_prefix("/"), "/api/v1");
    assert_eq!(protected_prefix(""), "/api/v1");
    assert_eq!(protected_prefix("/songbook"), "/songbook/api/v1");
}

mod access {
    use quench_auth::domain::jwt::Claims;
    use songbook_service::routers::api::gate::{Need, meets, need};
    use songbook_service::routers::ui::common::is_mobile;

    fn claims(scope: &str) -> Claims {
        Claims::for_audiences(
            "ann".into(),
            vec!["songbook".into()],
            scope.into(),
            None,
            900,
        )
    }

    #[quench_test::test]
    fn only_the_admin_paths_need_the_admin() {
        let api = "/songbook/api/v1";
        assert_eq!(need("/songbook/api/v1/admin/state", api), Need::Admin);
        assert_eq!(need("/songbook/api/v1/admin", api), Need::Admin);
        assert_eq!(need("/songbook/api/v1/library/songs", api), Need::Listener);
        assert_eq!(need("/songbook/api/v1/administrators", api), Need::Listener);
        assert_eq!(need("/songbook/api/v1/stream/4", api), Need::Listener);
    }

    #[quench_test::test]
    fn a_listener_is_not_an_admin_and_an_admin_may_listen() {
        let listener = claims("songbook:read");
        assert!(meets(&listener, "songbook", Need::Listener));
        assert!(!meets(&listener, "songbook", Need::Admin));
        let writer = claims("songbook:write");
        assert!(meets(&writer, "songbook", Need::Listener));
        assert!(meets(&writer, "songbook", Need::Admin));
        let someone_else = claims("sage:read");
        assert!(!meets(&someone_else, "songbook", Need::Listener));
        let wildcard = claims("admin");
        assert!(meets(&wildcard, "songbook", Need::Admin));
    }

    #[quench_test::test]
    fn phones_and_tablets_are_told_from_computers() {
        assert!(is_mobile(
            "Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 Chrome/126 Mobile Safari/537.36"
        ));
        assert!(is_mobile("Mozilla/5.0 (iPad; CPU OS 17_0 like Mac OS X)"));
        assert!(!is_mobile(
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 Chrome/126 Safari/537.36"
        ));
        assert!(!is_mobile(""));
    }
}

#[quench_test::test]
fn every_job_the_pages_start_is_known_to_the_service() {
    use songbook_service::routers::api::admin::JOBS;
    // The names the Pipeline, Schedule and Discover screens send (the purge preview is `purge` with `dry_run`).
    for name in [
        "sync",
        "tags",
        "similar",
        "youtube",
        "download",
        "convert",
        "images",
        "purge",
        "pipeline",
        "scheduled",
    ] {
        assert!(JOBS.contains(&name), "{name}");
    }
    assert!(!JOBS.contains(&"nothing"));
}
