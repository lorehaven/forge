//! Unit tests for `grounding/mod.rs` (the pure parts; the pipeline itself needs live services).

use sage_service::grounding::*;

fn hit(snippet: &str, url: &str) -> Hit {
    Hit {
        snippet: snippet.to_string(),
        url: url.to_string(),
    }
}

#[test]
fn parse_search_output_reads_header_snippets_and_urls() {
    let text = "Search results for 'rust' (via DuckDuckGo)\n\nRust is a language.\nSource: https://rust-lang.org/\n---\nCargo is the package manager.\nSource: https://doc.rust-lang.org/cargo";
    let hits = parse_search_output(text);
    assert_eq!(
        hits,
        vec![
            hit("Rust is a language.", "https://rust-lang.org/"),
            hit(
                "Cargo is the package manager.",
                "https://doc.rust-lang.org/cargo"
            ),
        ]
    );
}

#[test]
fn parse_search_output_skips_entries_without_a_usable_url() {
    let text = "Search results for 'x'\n\nno source here\n---\nbad\nSource: javascript:alert(1)\n---\nok\nSource: https://example.com";
    assert_eq!(
        parse_search_output(text),
        vec![hit("ok", "https://example.com")]
    );
}

#[test]
fn decode_redirect_unwraps_duckduckgo_links() {
    let href = "//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fa%3Fb%3D1&rut=abc";
    assert_eq!(decode_redirect(href), "https://example.com/a?b=1");
    assert_eq!(
        decode_redirect("https://example.com/x"),
        "https://example.com/x"
    );
    assert_eq!(decode_redirect("//example.com/x"), "https://example.com/x");
}

#[test]
fn merge_hits_round_robins_and_dedupes() {
    let merged = merge_hits(
        vec![
            vec![hit("a1", "https://a.com/1"), hit("a2", "https://a.com/2")],
            vec![hit("b1", "https://a.com/1/"), hit("b2", "https://b.com/2")],
        ],
        10,
    );
    let urls: Vec<_> = merged.iter().map(|h| h.url.as_str()).collect();
    assert_eq!(
        urls,
        ["https://a.com/1", "https://a.com/2", "https://b.com/2"]
    );
    assert_eq!(
        merge_hits(
            vec![vec![hit("a", "https://a.com"), hit("b", "https://b.com")]],
            1
        )
        .len(),
        1
    );
}

#[test]
fn parse_plan_handles_search_skip_and_garbage() {
    let plan = parse_plan(
        r#"{"needs_search":true,"queries":["  a  ","A","b","c","d"]}"#,
        "q",
        3,
    );
    assert!(plan.needs_search);
    assert_eq!(plan.queries, ["a", "b", "c"]);

    let none = parse_plan(r#"{"needs_search":false,"queries":["x"]}"#, "q", 3);
    assert!(!none.needs_search);
    assert!(none.queries.is_empty());

    let empty = parse_plan(r#"{"needs_search":true,"queries":[]}"#, "the question", 3);
    assert_eq!(empty.queries, ["the question"]);

    let garbage = parse_plan("not json", "the question", 3);
    assert!(garbage.needs_search);
    assert_eq!(garbage.queries, ["the question"]);
}

#[test]
fn cosine_of_parallel_orthogonal_and_zero_vectors() {
    assert!((cosine(&[1.0, 0.0], &[2.0, 0.0]) - 1.0).abs() < 1e-6);
    assert!(cosine(&[1.0, 0.0], &[0.0, 1.0]).abs() < 1e-6);
    assert_eq!(cosine(&[0.0, 0.0], &[1.0, 1.0]), 0.0);
}

#[test]
fn domain_of_strips_scheme_path_and_www() {
    assert_eq!(domain_of("https://www.example.com/a/b?c=d"), "example.com");
    assert_eq!(
        domain_of("http://sub.example.org:8080/x"),
        "sub.example.org:8080"
    );
}

#[test]
fn strip_invalid_citations_keeps_real_numbers_and_drops_invented_ones() {
    let text = "Rust is fast [1] and safe [2] (see [7]). Also [0].";
    assert_eq!(
        strip_invalid_citations(text, 2),
        "Rust is fast [1] and safe [2] (see). Also."
    );
    assert_eq!(strip_invalid_citations("No sources [1].", 0), "No sources.");
}

#[test]
fn strip_invalid_citations_ignores_indexing_and_code_fences() {
    let text = "Use items[1] here.\n```\nlet x = [3];\n```\nDone [3].";
    assert_eq!(
        strip_invalid_citations(text, 1),
        "Use items[1] here.\n```\nlet x = [3];\n```\nDone."
    );
}

#[test]
fn format_verdict_lists_unsupported_claims_under_the_models_notice() {
    let json = r#"{"notice":"Some parts could not be verified.","unsupported":[
        {"claim":"The race was on October 4, 2026","reason":"that date is in the future"},
        {"claim":"It was held at Sepang","reason":""}]}"#;
    assert_eq!(
        format_verdict(json).unwrap(),
        "\n\n⚠ Some parts could not be verified.\n- The race was on October 4, 2026 — that date is in the future\n- It was held at Sepang"
    );
}

#[test]
fn format_verdict_is_none_when_nothing_is_flagged_or_json_is_unusable() {
    assert!(format_verdict(r#"{"notice":"x","unsupported":[]}"#).is_none());
    assert!(
        format_verdict(r#"{"notice":"x","unsupported":[{"claim":"  ","reason":"r"}]}"#).is_none()
    );
    assert!(format_verdict("not json").is_none());
}

#[test]
fn format_verdict_falls_back_to_a_default_notice() {
    let out =
        format_verdict(r#"{"notice":"","unsupported":[{"claim":"c","reason":"r"}]}"#).unwrap();
    assert!(out.starts_with("\n\n⚠ Parts of this answer could not be verified"));
}
