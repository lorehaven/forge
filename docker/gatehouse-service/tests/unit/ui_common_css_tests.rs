use gatehouse_service::ui::common::ensure_gatehouse_css;

#[quench_test::test]
fn ensure_gatehouse_css_does_not_panic() {
    // `ui::common` (exercised by several other modules' tests via `render_page`/`ensure_assets`, all
    // sharing this crate's process) writes the same `dist/assets/css/gatehouse.css` with no locking
    // of its own, so this only asserts it runs cleanly - reading the file back would race those writers.
    ensure_gatehouse_css();
}
