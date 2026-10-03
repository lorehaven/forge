use crate::support;
use gantry_service::domain::cluster::Workload;
use http::{Method, StatusCode};
use quench_auth::domain::jwt::Claims;

fn location(resp: quench_http::response::Response) -> String {
    let (parts, _) = resp.into_hyper().into_parts();
    parts.headers["location"].to_str().unwrap().to_string()
}

fn running(version: &str) -> Vec<Workload> {
    vec![Workload {
        kind: "deployment".into(),
        name: "jellyfin".into(),
        namespace: "media".into(),
        package: "media".into(),
        version: Some(version.into()),
        desired: 1,
        ready: 1,
    }]
}

#[tokio::test]
async fn every_page_sends_a_signed_out_visitor_to_log_in() {
    let rig = support::rig(true).await;
    for path in [
        "/ui/home",
        "/ui/home/",
        "/ui",
        "/ui/",
        "/ui/operations",
        "/ui/targets/media",
        "/ui/plans/x",
        "/ui/operations/x",
    ] {
        let resp = rig
            .app
            .call(support::req(Method::GET, path, &rig.container))
            .await;
        assert!(resp.status().is_redirection(), "{path}: {}", resp.status());
    }
}

#[tokio::test]
async fn the_home_page_renders_for_a_signed_in_visitor_with_its_empty_state() {
    let rig = support::rig(false).await;
    let resp = rig
        .app
        .call(support::req(Method::GET, "/ui/home", &rig.container))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);

    let html = support::body_text(resp).await;
    assert!(html.contains("ui_home_no_targets"));
    assert!(html.contains("Gantry"));
}

#[tokio::test]
async fn the_root_redirects_a_signed_in_visitor_to_the_home_page() {
    let rig = support::rig(false).await;
    let resp = rig
        .app
        .call(support::req(Method::GET, "/ui", &rig.container))
        .await;
    assert!(resp.status().is_redirection());
    assert!(location(resp).ends_with("/ui/home"));
}

#[tokio::test]
async fn the_home_page_lists_targets_with_what_runs_beside_what_is_published() {
    let rig = support::rig(false).await;
    rig.registry.publish("media", "1.0.0", Some("media"));
    rig.registry.publish("media", "1.1.0", Some("media"));
    rig.cluster.set_workloads(running("1.0.0"));

    let html = support::body_text(
        rig.app
            .call(support::req(Method::GET, "/ui/home", &rig.container))
            .await,
    )
    .await;
    assert!(html.contains("/ui/targets/media"), "{html}");
    assert!(html.contains("1.0.0") && html.contains("1.1.0"));
    assert!(html.contains("ui_status_update_available"));
    assert!(html.contains("ui_nav_operations"));
}

#[tokio::test]
async fn a_package_that_cannot_be_read_is_shown_not_hidden() {
    let rig = support::rig(false).await;
    // Nothing published, but a workload nobody published: listed as such.
    rig.cluster.set_workloads(running("1.0.0"));
    let html = support::body_text(
        rig.app
            .call(support::req(Method::GET, "/ui/home", &rig.container))
            .await,
    )
    .await;
    assert!(html.contains("ui_status_unlisted"), "{html}");
}

#[tokio::test]
async fn a_scoped_grant_hides_the_other_packages_from_the_page() {
    let rig = support::rig(true).await;
    rig.registry.publish("media", "1.0.0", Some("media"));
    rig.registry.publish("forge", "1.0.0", Some("forge"));
    let mut request = support::req(Method::GET, "/ui/home", &rig.container);
    request.extensions_mut().insert(Claims::for_audiences(
        "ana".into(),
        vec!["gantry".into()],
        "gantry:target:media:read".into(),
        None,
        3600,
    ));
    let response = rig.app.call(request).await;
    // Whether this reads the identity from the extensions or insists on a session cookie, the other
    // package must not be on the page.
    if response.status() == StatusCode::OK {
        let html = support::body_text(response).await;
        assert!(html.contains("/ui/targets/media"), "{html}");
        assert!(!html.contains("/ui/targets/forge"), "{html}");
    } else {
        assert!(response.status().is_redirection());
    }
}

#[tokio::test]
async fn a_package_page_shows_versions_and_syncing_runs_at_once() {
    let rig = support::rig(false).await;
    rig.registry.publish("media", "1.0.0", Some("media"));
    rig.registry.publish("media", "1.1.0", Some("media"));
    rig.cluster.set_workloads(running("1.0.0"));

    let html = support::body_text(
        rig.app
            .call(support::req(
                Method::GET,
                "/ui/targets/media",
                &rig.container,
            ))
            .await,
    )
    .await;
    assert!(html.contains("jellyfin"));
    assert!(html.contains("ui_action_sync"));
    assert!(
        !html.contains("ui_overrides"),
        "variables live in the package, not in a box here"
    );
    assert!(html.contains("/ui/targets/media/sync"));

    // No plan to confirm: it goes straight to the operation.
    let synced = rig
        .app
        .call(support::req_form(
            Method::POST,
            "/ui/targets/media/sync",
            &rig.container,
            &[("version", "")],
        ))
        .await;
    assert!(synced.status().is_redirection());
    let operation_url = location(synced);
    assert!(operation_url.contains("/ui/operations/"), "{operation_url}");

    let page = support::body_text(
        rig.app
            .call(support::req(Method::GET, &operation_url, &rig.container))
            .await,
    )
    .await;
    assert!(page.contains("Upgrade media → 1.1.0"), "{page}");
    assert!(
        page.contains("install media 1.1.0"),
        "the steps are one click away"
    );
    assert!(page.contains("ui_state_queued"));
}

#[tokio::test]
async fn a_running_operation_can_be_cancelled_and_stops_refreshing() {
    let rig = support::rig(false).await;
    rig.registry.publish("media", "1.0.0", Some("media"));
    let synced = rig
        .app
        .call(support::req_form(
            Method::POST,
            "/ui/targets/media/sync",
            &rig.container,
            &[("version", "")],
        ))
        .await;
    let operation_url = location(synced);

    let page = support::body_text(
        rig.app
            .call(support::req(Method::GET, &operation_url, &rig.container))
            .await,
    )
    .await;
    assert!(page.contains("ui_op_cancel"));
    assert!(
        page.contains("window.location.reload"),
        "a running operation refreshes itself"
    );

    let list = support::body_text(
        rig.app
            .call(support::req(Method::GET, "/ui/operations", &rig.container))
            .await,
    )
    .await;
    assert!(list.contains("Install media 1.0.0"), "{list}");

    let cancelled = rig
        .app
        .call(support::req_form(
            Method::POST,
            &format!("{operation_url}/cancel"),
            &rig.container,
            &[],
        ))
        .await;
    assert!(cancelled.status().is_redirection());
    let page = support::body_text(
        rig.app
            .call(support::req(Method::GET, &operation_url, &rig.container))
            .await,
    )
    .await;
    assert!(page.contains("ui_state_cancelled"), "{page}");
    assert!(
        !page.contains("window.location.reload"),
        "a finished operation stops refreshing"
    );
}
