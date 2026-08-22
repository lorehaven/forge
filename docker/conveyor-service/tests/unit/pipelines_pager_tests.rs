use chrono::Utc;
use conveyor_service::domain::Project;
use conveyor_service::routers::ui::pages::pipelines::{page_count, pager};

fn project(id: &str) -> Project {
    Project {
        id: id.to_string(),
        name: id.to_string(),
        parent_id: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

#[test]
fn an_empty_history_is_still_page_one_of_one() {
    assert_eq!(page_count(0, 25), 1);
}

#[test]
fn a_partial_last_page_still_counts_as_a_whole_page() {
    assert_eq!(page_count(26, 25), 2);
    assert_eq!(page_count(25, 25), 1);
    assert_eq!(page_count(50, 25), 2);
}

#[test]
fn the_first_page_has_no_previous_link() {
    let html = pager(None, 1, 3).render();
    assert!(html.contains("pager-link-disabled"));
    assert!(!html.contains("page=0"));
}

#[test]
fn the_last_page_has_no_next_link() {
    let html = pager(None, 3, 3).render();
    let next_disabled = html.rfind("pager-link-disabled").expect("a disabled link");
    // Only the trailing (next) control should be disabled on the last page.
    assert!(html[..next_disabled].contains("page=2"));
}

#[test]
fn a_middle_page_links_both_ways() {
    let html = pager(None, 2, 3).render();
    assert!(html.contains("page=1"));
    assert!(html.contains("page=3"));
    assert!(!html.contains("pager-link-disabled"));
}

#[test]
fn a_scoped_pager_carries_the_project_along() {
    let scope = project("lorehaven");
    let html = pager(Some(&scope), 1, 2).render();
    assert!(html.contains("project=lorehaven"));
    assert!(html.contains("page=2"));
}
