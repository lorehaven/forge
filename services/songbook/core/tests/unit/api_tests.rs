use songbook_core::api::encode;
use songbook_core::{Filter, JobStart};

#[quench_test::test]
fn a_filter_becomes_a_query_without_its_empty_fields() {
    assert_eq!(Filter::default().query(), "");
    let f = Filter {
        artist: "Pięć Dwa".into(),
        q: "a&b".into(),
        links: "missing".into(),
        ..Filter::default()
    };
    assert_eq!(
        f.query(),
        "?artist=Pi%C4%99%C4%87%20Dwa&q=a%26b&links=missing"
    );
}

#[quench_test::test]
fn encoding_keeps_unreserved_characters() {
    assert_eq!(encode("a-b_c.d~e"), "a-b_c.d~e");
    assert_eq!(encode("a b/c"), "a%20b%2Fc");
}

#[quench_test::test]
fn a_job_start_reads_with_missing_fields() {
    let j: JobStart = quench_serde::json::from_str(r#"{"name":"sync"}"#).unwrap();
    assert_eq!(j.name, "sync");
    assert!(!j.singles && j.limit.is_none() && j.limit_download.is_none());
}
