//! What the browser half adds to the app: where the service's prefix comes from, and what the stylesheet carries.

use genevia_app::api::{base_of, segment};

#[quench_test::test]
fn the_prefix_is_whatever_comes_before_ui() {
    assert_eq!(base_of("/genevia/ui"), "/genevia");
    assert_eq!(base_of("/genevia/ui/t/kowal/list/I1"), "/genevia");
    assert_eq!(base_of("/ui"), "");
    assert_eq!(base_of("/ui/t/kowal"), "");
    assert_eq!(base_of("/a/b/ui/t/x"), "/a/b");
    assert_eq!(base_of("/elsewhere"), "");
}

#[quench_test::test]
fn path_segments_are_encoded() {
    assert_eq!(segment("I12"), "I12");
    assert_eq!(segment("@I1@"), "%40I1%40");
    assert_eq!(segment("jan kowal/ł"), "jan%20kowal%2F%C5%82");
}

#[quench_test::test]
fn the_page_carries_the_apps_own_rules() {
    let css = genevia_web::stylesheet();
    assert!(css.contains(".gen-toolbar"));
    assert!(css.contains("var(--fui-"), "written on the forge tokens");
}

quench_test::main!();
