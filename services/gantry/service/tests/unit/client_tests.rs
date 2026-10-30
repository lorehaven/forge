//! The routes that start the browser client: the page behind the session check, and the files it loads.

use crate::support;
use quench_http::http::{Method, StatusCode};

async fn get(path: &str, rig: &support::Rig) -> (StatusCode, String) {
    let response = rig
        .app
        .call(support::req(Method::GET, path, &rig.container))
        .await;
    let status = response.status();
    (status, support::body_text(response).await)
}

/// The page needs the built module (`web/app.wasm`): without it the service says so, which still shows that the visitor got
/// past the session check. `forge-web`'s own tests cover the page itself.
#[quench_test::test]
async fn the_pages_start_the_client_for_a_signed_in_visitor() {
    let rig = support::rig(false).await;
    for path in [
        "/ui/home",
        "/ui/home/",
        "/ui/apps/media",
        "/ui/targets/media",
        "/ui/operations",
        "/ui/operations/o1",
        "/ui/plans/p1",
        "/ui/resource?package=ml&kind=ConfigMap&name=x",
    ] {
        let (status, body) = get(path, &rig).await;
        if status == StatusCode::OK {
            assert!(body.contains("/ui/assets/quench.js"), "{path}: {body}");
        } else {
            assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
            assert!(body.contains("not installed"), "{path}: {body}");
        }
    }
}

#[quench_test::test]
async fn a_visitor_with_no_session_is_sent_to_sign_in() {
    let rig = support::rig(true).await;
    for path in [
        "/ui",
        "/ui/",
        "/ui/home",
        "/ui/home/",
        "/ui/apps/media",
        "/ui/targets/media",
        "/ui/operations",
        "/ui/operations/o1",
        "/ui/plans/p1",
        "/ui/resource?package=ml&kind=ConfigMap&name=x",
    ] {
        let (status, _) = get(path, &rig).await;
        assert!(status.is_redirection(), "{path}: {status}");
    }
}

#[quench_test::test]
async fn the_root_sends_a_signed_in_visitor_to_the_packages() {
    let rig = support::rig(false).await;
    let response = rig
        .app
        .call(support::req(Method::GET, "/ui", &rig.container))
        .await;
    assert!(response.status().is_redirection());
    let parts = response.into_inner();
    let location = parts.headers.get("location").unwrap().to_str().unwrap();
    assert!(location.ends_with("/ui/home"), "{location}");
}

#[quench_test::test]
async fn the_files_are_public_and_the_module_may_be_missing() {
    let rig = support::rig(true).await;
    let (status, body) = get("/ui/assets/app.css", &rig).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains(".gt-app"), "{body}");
    let (status, body) = get("/ui/assets/quench.js", &rig).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!body.is_empty());
    let (status, _) = get("/ui/assets/app.wasm", &rig).await;
    assert!(
        status == StatusCode::OK || status == StatusCode::NOT_FOUND,
        "{status}"
    );
}
