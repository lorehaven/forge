//! `supported_locales` in
//! `routers/ui/common/mod.rs`, plus a light smoke test of the `assets`
//! handler's 404 path (no `dist/assets` fixture is set up for a real file).

use quench_http::prelude::Path;
use switchboard_service::routers::ui::common::{assets, supported_locales};

#[test]
fn supported_locales_lists_all_five_configured_locales() {
    let locales = supported_locales();
    assert_eq!(locales.len(), 5);
    assert!(locales.contains(&"en-US".to_string()));
    assert!(locales.contains(&"pl-PL".to_string()));
}

#[tokio::test]
async fn assets_returns_not_found_for_a_nonexistent_asset() {
    let resp = assets(Path("does-not-exist.css".to_string())).await;
    assert!(!resp.status().is_success());
}
