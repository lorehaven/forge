//! Unit tests for `routers/ui/common/mod.rs`: the estate-wide status pill.

use conveyor_service::domain::Status;
use conveyor_service::routers::ui::common::status_pill;

#[test]
fn every_status_gets_its_own_class_and_translation_key() {
    for status in [
        Status::Queued,
        Status::Running,
        Status::Success,
        Status::Failed,
        Status::Cancelled,
        Status::Skipped,
    ] {
        let html = status_pill(status).render();
        assert!(html.contains("status"), "{status:?}: {html}");
        assert!(
            html.contains(&format!("status-{status}")),
            "{status:?}: {html}"
        );
        assert!(
            html.contains(&format!("ui_status_{status}")),
            "{status:?}: {html}"
        );
    }
}
