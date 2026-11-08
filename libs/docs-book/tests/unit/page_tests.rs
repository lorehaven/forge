use docs_book::{parse_page, split_front_matter, strip_front_matter};

#[quench_test::test]
fn front_matter_is_split_from_the_body() {
    assert_eq!(
        split_front_matter("---\ntitle: A\n---\nbody\n"),
        Some(("title: A\n", "body\n"))
    );
    assert_eq!(split_front_matter("---\n---\nbody\n"), Some(("", "body\n")));
    assert_eq!(split_front_matter("no front matter\n"), None);
    assert_eq!(strip_front_matter("---\ntitle: A\n---\n# A\n"), "# A\n");
    assert_eq!(strip_front_matter("# Just text\n"), "# Just text\n");
}

#[quench_test::test]
fn a_page_reads_its_navigation_settings() {
    let page = parse_page(
        "services/gauge/metrics.md",
        "---\ntitle: Metrics\nparent: Gauge\ngrand_parent: Services\nnav_order: 7\nhas_children: true\nnav_exclude: true\npermalink: /gauge/metrics/\n---\nText\n",
    )
    .unwrap();
    assert_eq!(page.title.as_deref(), Some("Metrics"));
    assert_eq!(page.parent.as_deref(), Some("Gauge"));
    assert_eq!(page.grand_parent.as_deref(), Some("Services"));
    assert_eq!(page.nav_order, Some(7));
    assert!(page.has_children && page.nav_exclude);
    assert_eq!(page.permalink.as_deref(), Some("/gauge/metrics/"));
    assert_eq!(page.body, "Text\n");
    assert_eq!(page.route(), "services/gauge/metrics");
}

#[quench_test::test]
fn routes_drop_the_extension_and_index() {
    let route = |path: &str| {
        parse_page(path, "---\ntitle: x\n---\n")
            .unwrap()
            .route()
            .to_string()
    };
    assert_eq!(route("index.md"), "");
    assert_eq!(route("services/index.md"), "services");
    assert_eq!(route("guides/testing.md"), "guides/testing");
}

#[quench_test::test]
fn a_file_without_front_matter_or_with_bad_yaml_is_not_a_page() {
    let missing = parse_page("notes.md", "# Notes\n").unwrap_err();
    assert!(missing.contains("no front matter"), "{missing}");
    let bad = parse_page("bad.md", "---\ntitle: [unclosed\n---\n").unwrap_err();
    assert!(bad.contains("not valid YAML"), "{bad}");
}

#[quench_test::test]
fn the_label_falls_back_to_the_file_name() {
    let page = parse_page("guides/deploy.md", "---\nnav_order: 1\n---\n").unwrap();
    assert_eq!(page.label(), "deploy");
}
