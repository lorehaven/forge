//! Unit tests for `actix/routers/ui/mod.rs`.
//!
//! The two forms of "you are not signed in" have to differ, because the two
//! callers cannot act on the same answer: a browser follows `Location`, and
//! htmx never sees it.

use actix_web::http::StatusCode;
use actix_web::test::TestRequest;
use quench_starter::actix::routers::ui::{ui_login_redirect, ui_login_redirect_for};

#[test]
fn a_page_request_is_redirected_the_ordinary_way() {
    let request = TestRequest::default().to_http_request();
    let response = ui_login_redirect_for(&request);

    assert_eq!(response.status(), StatusCode::FOUND);
    assert!(response.headers().contains_key("location"));
    assert!(!response.headers().contains_key("hx-redirect"));
}

#[test]
fn a_fragment_request_is_told_to_navigate() {
    // XHR follows a 302 below the point htmx can see it, so a fragment answered
    // that way swaps the login page into whatever asked. `HX-Redirect` is the
    // only form that reaches the browser's address bar.
    let request = TestRequest::default()
        .insert_header(("HX-Request", "true"))
        .to_http_request();
    let response = ui_login_redirect_for(&request);

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "htmx does not read the headers of a response it treats as an error"
    );

    let target = response
        .headers()
        .get("hx-redirect")
        .expect("a fragment must be told where to go")
        .to_str()
        .expect("a header that is text");
    assert!(target.ends_with("/ui/login"), "got: {target}");
}

#[test]
fn both_forms_name_the_same_destination() {
    let plain = ui_login_redirect();
    let location = plain
        .headers()
        .get("location")
        .and_then(|value| value.to_str().ok())
        .expect("a location")
        .to_string();

    let request = TestRequest::default()
        .insert_header(("HX-Request", "true"))
        .to_http_request();
    let fragment = ui_login_redirect_for(&request);
    let redirect = fragment
        .headers()
        .get("hx-redirect")
        .and_then(|value| value.to_str().ok())
        .expect("a redirect");

    assert_eq!(location, redirect);
}
