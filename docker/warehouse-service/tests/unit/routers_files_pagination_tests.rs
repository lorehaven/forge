use warehouse_service::routers::files::pagination::{next_link, page_size, paginate, resume_after};

#[test]
fn page_size_uses_the_default_when_nothing_is_requested() {
    assert_eq!(page_size(None, 200, 1000), 200);
}

#[test]
fn page_size_clamps_a_request_below_the_minimum() {
    assert_eq!(page_size(Some(0), 200, 1000), 1);
}

#[test]
fn page_size_clamps_a_request_above_the_maximum() {
    assert_eq!(page_size(Some(50_000), 200, 1000), 1000);
}

#[test]
fn paginate_reports_no_more_when_everything_fit() {
    let page = paginate(vec![1, 2, 3], 3);
    assert_eq!(page.items, vec![1, 2, 3]);
    assert!(!page.has_more);
}

#[test]
fn paginate_truncates_and_reports_more_when_it_did_not() {
    let page = paginate(vec![1, 2, 3, 4], 3);
    assert_eq!(page.items, vec![1, 2, 3]);
    assert!(page.has_more);
}

#[test]
fn resume_after_none_starts_at_the_beginning() {
    let items = ["alpha", "beta", "gamma"];
    assert_eq!(resume_after(&items, None, |s| s), 0);
}

#[test]
fn resume_after_a_known_key_starts_just_past_it() {
    let items = ["alpha", "beta", "gamma"];
    assert_eq!(resume_after(&items, Some("beta"), |s| s), 2);
}

#[test]
fn resume_after_an_unknown_key_restarts_from_the_beginning() {
    let items = ["alpha", "beta", "gamma"];
    assert_eq!(resume_after(&items, Some("nonexistent"), |s| s), 0);
}

#[test]
fn next_link_percent_encodes_the_cursor() {
    let link = next_link("/api/v1/files/backups?prefix=photos", 200, "photos/a b.jpg");
    assert_eq!(
        link,
        "</api/v1/files/backups?prefix=photos&n=200&last=photos%2Fa%20b.jpg>; rel=\"next\""
    );
}
