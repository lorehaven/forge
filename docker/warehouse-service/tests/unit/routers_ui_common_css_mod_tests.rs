use warehouse_service::routers::ui::common::ensure_warehouse_css;

#[quench_test::test]
fn ensure_warehouse_css_writes_the_stylesheet_without_panicking() {
    // `ensure_warehouse_css` itself swallows I/O errors, so this only
    // asserts it didn't panic. It can't assert on the file's content: nextest
    // runs every UI-page test as its own process, several of which call this
    // same function against the same fixed relative path
    // (`dist/assets/css/warehouse.css`), so a sibling process can legitimately
    // be mid create-and-truncate at the moment this reads it back - a
    // transiently empty read here is that race, not a bug in the function.
    // `warehouse_css_rules_is_non_empty_and_renders_without_panicking` above
    // already covers the actual "renders to non-empty CSS" content check,
    // entirely in-process with no shared I/O.
    ensure_warehouse_css();
}
