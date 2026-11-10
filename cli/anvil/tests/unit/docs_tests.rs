use std::cell::RefCell;
use std::path::{Path, PathBuf};

use anvil::commands::artifact::Upload;
use anvil::commands::docs::{
    Options, check_book, destinations, links, normalise_route, openapi_problems, release_with,
    source_routes, split_front_matter,
};
use anvil::config::Config;

fn temp(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("anvil-docs-{}-{label}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(root: &Path, path: &str, text: &str) {
    let file = root.join(path);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, text).unwrap();
}

fn page(title: &str, extra: &str, body: &str) -> String {
    format!("---\ntitle: {title}\n{extra}---\n\n{body}\n")
}

fn config(toml: &str) -> Config {
    quench_serde::toml::from_str(toml).unwrap()
}

const BOOK: &str = r#"
[docs]
storage = "docs"
warehouse_url = "https://warehouse.example/warehouse/"
[docs.books.demo]
dir = "docs"
title = "Demo"
version = "1.2.3"
"#;

#[quench_test::test]
fn front_matter_is_split_from_the_body() {
    let (head, body) = split_front_matter("---\ntitle: A\n---\nbody\n").unwrap();
    assert_eq!(head, "title: A\n");
    assert_eq!(body, "body\n");
    assert!(split_front_matter("no front matter\n").is_none());
    assert!(split_front_matter("---\ntitle: A\nnever closed\n").is_none());
}

#[quench_test::test]
fn links_in_code_are_not_links() {
    let body = "See [a](a.md) and [b](dir/b.md#x).\n```text\n[no](nope.md)\n```\nInline `[no](nope.md)` too, but [c](c.md).\n";
    assert_eq!(links(body), ["a.md", "dir/b.md#x", "c.md"]);
}

#[quench_test::test]
fn routes_are_compared_in_one_spelling() {
    assert_eq!(normalise_route("/x/:id/y"), "/x/{id}/y");
    assert_eq!(
        normalise_route("/v2/{name:.+}/blobs/{digest}"),
        "/v2/{name}/blobs/{digest}"
    );
    assert_eq!(normalise_route("/plain"), "/plain");
}

#[quench_test::test]
fn routes_come_from_macros_and_from_hand_registrations() {
    let source = r#"
        #[get("/api/v1/things/{id}")]
        async fn one() {}
        #[ post ( "/api/v1/things" ) ]
        async fn make() {}
        const A: RouteRegistration = RouteRegistration { method: Method::HEAD, pattern: "/v2/{name:.+}/x" };
        #[derive(Debug)]
        struct NotARoute;
    "#;
    let found = source_routes(source);
    let found: Vec<(&str, &str)> = found
        .iter()
        .map(|(m, p)| (m.as_str(), p.as_str()))
        .collect();
    assert_eq!(
        found,
        [
            ("get", "/api/v1/things/{id}"),
            ("head", "/v2/{name}/x"),
            ("post", "/api/v1/things")
        ]
    );
}

#[quench_test::test]
fn a_good_book_has_no_problems() {
    let root = temp("good");
    write(
        &root,
        "docs/index.md",
        &page("Demo", "nav_order: 1\nhas_children: true\n", "[a](a.md)"),
    );
    write(
        &root,
        "docs/a.md",
        &page(
            "A",
            "parent: Demo\nnav_order: 1\n",
            "[home](index.md) [dir](sub/)",
        ),
    );
    write(
        &root,
        "docs/sub/index.md",
        &page("Sub", "parent: Demo\nnav_order: 2\n", ""),
    );
    write(&root, "docs/_private.md", "not a page, ignored\n");
    let config = config(BOOK);
    let problems = check_book(&root, &config.docs.books["demo"]).unwrap();
    assert!(problems.is_empty(), "{problems:?}");
}

#[quench_test::test]
fn a_broken_book_names_every_problem() {
    let root = temp("bad");
    write(
        &root,
        "docs/index.md",
        &page("Demo", "has_children: true\n", "[gone](gone.md)"),
    );
    write(&root, "docs/notitle.md", "---\nnav_order: 1\n---\nbody\n");
    write(
        &root,
        "docs/orphan.md",
        &page("Orphan", "parent: Nobody\n", ""),
    );
    write(&root, "docs/childless.md", &page("Childless", "", ""));
    write(
        &root,
        "docs/under.md",
        &page("Under", "parent: Childless\n", ""),
    );
    write(
        &root,
        "docs/one.md",
        &page("One", "parent: Demo\nnav_order: 5\n", ""),
    );
    write(
        &root,
        "docs/two.md",
        &page("Two", "parent: Demo\nnav_order: 5\n", ""),
    );
    write(&root, "docs/bare.md", "no front matter\n");
    write(&root, "docs/yaml.md", "---\ntitle: a: b: c\n---\n");
    let config = config(BOOK);
    let problems = check_book(&root, &config.docs.books["demo"])
        .unwrap()
        .join("\n");
    for expected in [
        "index.md: broken link `gone.md`",
        "notitle.md: no `title`",
        "orphan.md: parent `Nobody` is not the title of any page",
        "under.md: parent `Childless` exists but has no `has_children: true`",
        "nav_order 5 is used by 2 pages under Demo",
        "bare.md: no front matter",
        "yaml.md: the front matter is not valid YAML",
    ] {
        assert!(
            problems.contains(expected),
            "missing `{expected}` in:\n{problems}"
        );
    }
}

#[quench_test::test]
fn an_openapi_document_must_match_the_routes() {
    let root = temp("openapi");
    write(
        &root,
        "src/lib.rs",
        "#[get(\"/a\")]\nasync fn a() {}\n#[post(\"/b/:id\")]\nasync fn b() {}\n",
    );
    let spec = r##"
openapi: 3.1.0
info: {title: T, version: "1"}
paths:
  /a:
    get: {responses: {"200": {description: ok, content: {application/json: {schema: {$ref: "#/components/schemas/Missing"}}}}}}
  /b/{id}:
    post: {responses: {"200": {description: ok}}}
  /stale:
    get: {responses: {"200": {description: ok}}}
"##;
    let sources = [root.join("src")];
    let problems = openapi_problems("demo", spec, &sources).join("\n");
    assert!(
        problems.contains("documented but no route in source: GET /stale"),
        "{problems}"
    );
    assert!(
        problems.contains("dangling $ref #/components/schemas/Missing"),
        "{problems}"
    );
    assert!(!problems.contains("/b/{id}"), "{problems}");

    let short = "openapi: 3.1.0\ninfo: {title: T, version: '1'}\npaths:\n  /a:\n    get: {}\n";
    let problems = openapi_problems("demo", short, &sources).join("\n");
    assert!(
        problems.contains("route in source but not documented: POST /b/{id}"),
        "{problems}"
    );
    let old = "openapi: 3.0.3\ninfo: {title: T, version: '1'}\npaths: {}\n";
    assert!(
        openapi_problems("demo", old, &sources)
            .join("\n")
            .contains("must be 3.1.x")
    );
}

#[cfg(unix)]
fn demo(label: &str) -> (PathBuf, Config) {
    let root = temp(label);
    write(
        &root,
        "docs/index.md",
        &page("Demo", "nav_order: 1\n", "hello"),
    );
    (root, config(BOOK))
}

#[cfg(unix)]
#[quench_test::test]
fn a_release_sends_the_archive_the_manifest_and_the_latest_pointer() {
    let (root, config) = demo("release");
    let sent: RefCell<Vec<Upload>> = RefCell::default();
    let mut log = Vec::new();
    release_with(
        &config,
        &root,
        Some("demo"),
        &Options::default(),
        &mut log,
        &|| Ok("tok".to_string()),
        &|u| {
            sent.borrow_mut().push(u.clone());
            Ok((201, String::new()))
        },
    )
    .unwrap();
    let sent = sent.into_inner();
    let urls: Vec<&str> = sent.iter().map(|u| u.url.as_str()).collect();
    assert_eq!(
        urls,
        [
            "https://warehouse.example/warehouse/api/v1/files/docs/file?path=demo/1.2.3/demo-1.2.3.tar.gz",
            "https://warehouse.example/warehouse/api/v1/files/docs/file?path=demo/1.2.3/manifest.json",
            "https://warehouse.example/warehouse/api/v1/files/docs/file?path=demo/latest.json",
        ]
    );
    assert!(sent.iter().all(|u| u.token == "tok"));
    assert_eq!(sent[0].content_type, "application/gzip");
    // The archive is a gzip stream (its magic bytes), the manifests are JSON that name each other.
    assert_eq!(&sent[0].body[..2], &[0x1f, 0x8b]);
    let manifest = String::from_utf8(sent[1].body.clone()).unwrap();
    assert!(
        manifest.contains("\"book\": \"demo\"") && manifest.contains("\"files\": 4"),
        "{manifest}"
    );
    let latest = String::from_utf8(sent[2].body.clone()).unwrap();
    assert!(
        latest.contains("\"archive_path\": \"demo/1.2.3/demo-1.2.3.tar.gz\""),
        "{latest}"
    );
    let text = String::from_utf8(log).unwrap();
    assert!(text.contains("released demo 1.2.3"), "{text}");
    // The same book built twice gives the same archive bytes (the sha256 in the manifest is stable).
    let first = std::fs::read(root.join("target/docs/demo/demo-1.2.3.tar.gz")).unwrap();
    release_with(
        &config,
        &root,
        Some("demo"),
        &Options {
            dry_run: true,
            ..Options::default()
        },
        &mut Vec::new(),
        &|| panic!("a dry run asks for no token"),
        &|_| panic!("a dry run sends nothing"),
    )
    .unwrap();
    assert_eq!(
        first,
        std::fs::read(root.join("target/docs/demo/demo-1.2.3.tar.gz")).unwrap()
    );
}

#[cfg(unix)]
#[quench_test::test]
fn a_refused_upload_says_what_to_do() {
    let (root, config) = demo("refused");
    for (status, expected) in [
        (403, "anvil login"),
        (404, "no files storage `docs`"),
        (500, "answered 500"),
    ] {
        let err = release_with(
            &config,
            &root,
            None,
            &Options::default(),
            &mut Vec::new(),
            &|| Ok("tok".to_string()),
            &|_| Ok((status, "boom".to_string())),
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains(expected), "{status}: {err}");
    }
}

#[quench_test::test]
fn a_release_needs_a_warehouse_and_a_storage() {
    let root = temp("nowh");
    let config = config("[docs.books.demo]\ndir = \"docs\"\n");
    let err = release_with(
        &config,
        &root,
        None,
        &Options::default(),
        &mut Vec::new(),
        &|| Ok(String::new()),
        &|_| Ok((201, String::new())),
    )
    .unwrap_err()
    .to_string();
    assert!(err.contains("no warehouse"), "{err}");
}

#[quench_test::test]
fn the_destinations_are_book_version_and_latest() {
    let built = anvil::commands::docs::Built {
        book: "forge".into(),
        version: "0.1.0".into(),
        site: PathBuf::new(),
        archive: PathBuf::from("/x/forge-0.1.0.tar.gz"),
        manifest: PathBuf::new(),
        files: 1,
        bytes: 1,
        manifest_text: String::new(),
    };
    assert_eq!(
        destinations(&built),
        [
            "forge/0.1.0/forge-0.1.0.tar.gz",
            "forge/0.1.0/manifest.json",
            "forge/latest.json"
        ]
    );
}

#[quench_test::test]
fn the_site_links_pages_to_the_pages_it_writes() {
    let (root, config) = demo("site");
    write(
        &root,
        "docs/other.md",
        &page(
            "Other",
            "nav_order: 2\n",
            "[home](index.md) [part](index.md#top-part) [out](https://x.example/d.md) [file](data.json)\n\n## Top part\n\n```rust\nfn x() {}\n```\n\n| a | b |\n|---|---|\n| 1 | 2 |\n",
        ),
    );
    write(&root, "docs/data.json", "{}");
    anvil::commands::docs::build(
        &config,
        &root,
        Some("demo"),
        &Options::default(),
        &mut Vec::new(),
    )
    .unwrap();
    let site = root.join("target/docs/demo/site");
    let html = std::fs::read_to_string(site.join("other.html")).unwrap();
    assert!(html.contains("href=\"/\">home"), "{html}");
    assert!(html.contains("href=\"/#top-part\">part"), "{html}");
    assert!(html.contains("href=\"https://x.example/d.md\""), "{html}");
    assert!(html.contains("href=\"data.json\""), "{html}");
    assert!(html.contains("<h2 id=\"top-part\">Top part"), "{html}");
    assert!(
        html.contains("class=\"language-rust highlighter-rouge\""),
        "{html}"
    );
    assert!(
        html.contains("<div class=\"table-wrapper\"><table>"),
        "{html}"
    );
    // the navigation lists both pages, and this one is the current one
    assert!(html.contains("class=\"nav-list-link active\""), "{html}");
    assert!(site.join("index.html").is_file());
    assert!(site.join("data.json").is_file(), "other files are copied");
    let search = std::fs::read_to_string(site.join("assets/js/search-data.json")).unwrap();
    assert!(
        search.contains("\"title\": \"Top part\"")
            && search.contains("\"url\": \"/other.html#top-part\""),
        "{search}"
    );
    // The book itself is untouched.
    let original = std::fs::read_to_string(root.join("docs/other.md")).unwrap();
    assert!(original.contains("[home](index.md)"), "{original}");
}

#[quench_test::test]
fn a_baseurl_goes_in_front_of_every_address() {
    let (root, config) = demo("baseurl");
    write(&root, "docs/_config.yml", "title: Demo\nbaseurl: /demo\n");
    anvil::commands::docs::build(
        &config,
        &root,
        Some("demo"),
        &Options::default(),
        &mut Vec::new(),
    )
    .unwrap();
    let html = std::fs::read_to_string(root.join("target/docs/demo/site/index.html")).unwrap();
    assert!(
        html.contains("href=\"/demo/assets/css/site.css\""),
        "{html}"
    );
    assert!(html.contains("href=\"/demo/\""), "{html}");
}

#[quench_test::test]
fn heading_anchors_are_written_the_way_the_links_to_them_are() {
    use anvil::commands::site::heading_slug;
    assert_eq!(heading_slug("loupe --book"), "loupe---book");
    assert_eq!(
        heading_slug("The scheduler (forklift_core::Engine)"),
        "the-scheduler-forklift_coreengine"
    );
    assert_eq!(
        heading_slug("`[[stage.job.service]]` sidecar containers"),
        "stagejobservice-sidecar-containers"
    );
    assert_eq!(heading_slug("???"), "section");
}

#[quench_test::test]
fn a_heading_may_name_its_own_anchor() {
    let html = anvil::commands::site::decorated("## Help templates {#help-templates}\n");
    assert!(
        html.contains("<h2 id=\"help-templates\">Help templates<a"),
        "{html}"
    );
}

#[quench_test::test]
fn repeated_headings_get_numbered_anchors() {
    let html = anvil::commands::site::decorated("## Same\n\n## Same\n");
    assert!(
        html.contains("id=\"same\"") && html.contains("id=\"same-1\""),
        "{html}"
    );
}

#[quench_test::test]
fn the_forge_theme_is_the_forge_tokens_and_a_script_without_line_comments() {
    let (root, config) = demo("theme");
    let mut log = Vec::new();
    anvil::commands::docs::build(&config, &root, Some("demo"), &Options::default(), &mut log)
        .unwrap();
    let site = root.join("target/docs/demo/site");
    let css = std::fs::read_to_string(site.join("assets/css/site.css")).unwrap();
    assert!(
        css.contains("--fui-brand-500") && css.contains(".nav-list"),
        "tokens, then rules"
    );
    let html = std::fs::read_to_string(site.join("index.html")).unwrap();
    assert!(
        html.contains("fui-dropdown") && html.contains("fui-brand-mark"),
        "the theme menu and brand"
    );

    let none = Config {
        docs: anvil::config::DocsConfig {
            theme: Some("none".to_string()),
            ..config.docs
        },
        ..config
    };
    anvil::commands::docs::build(&none, &root, Some("demo"), &Options::default(), &mut log)
        .unwrap();
    let css = std::fs::read_to_string(site.join("assets/css/site.css")).unwrap();
    assert!(!css.contains("--fui-brand-500"), "layout alone");
    let html = std::fs::read_to_string(site.join("index.html")).unwrap();
    assert!(!html.contains("fui-dropdown"));

    let bad = Config {
        docs: anvil::config::DocsConfig {
            theme: Some("neon".to_string()),
            ..none.docs
        },
        ..none
    };
    let err =
        anvil::commands::docs::build(&bad, &root, Some("demo"), &Options::default(), &mut log)
            .unwrap_err()
            .to_string();
    assert!(err.contains("neon"), "{err}");
}

#[quench_test::test]
fn the_tokens_are_the_ones_of_the_forge_services() {
    let shell =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../libs/forge-shell/assets/tokens.css");
    let ours = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/docs-theme/forge-tokens.css"),
    )
    .unwrap();
    // Only inside the forge repository: a published copy of anvil has no forge-shell beside it.
    if let Ok(original) = std::fs::read_to_string(shell) {
        assert_eq!(ours.lines().next(), original.lines().next());
        assert!(
            ours.contains(&original),
            "assets/docs-theme/forge-tokens.css differs from libs/forge-shell/assets/tokens.css: copy it again"
        );
    }
}

/// The paths in a release archive.
fn archive_paths(archive: &[u8]) -> Vec<String> {
    let tar = quench_archive::gzip::decompress(archive, 1 << 26).unwrap();
    quench_archive::tar::Reader::new(&tar)
        .map(|entry| entry.unwrap().path)
        .collect()
}

#[cfg(unix)]
#[quench_test::test]
fn the_archive_carries_the_markdown_sources_under_source() {
    let (root, config) = demo("sources");
    write(&root, "docs/_config.yml", "title: Demo\nbaseurl: /demo\n");
    write(
        &root,
        "docs/guide/setup.md",
        &page(
            "Setup",
            "nav_order: 2\n",
            "![shot](shot.png) ![logo](/assets/logo.png) ![based]({{ site.baseurl }}/assets/based.png)\n\
             ![far](https://example.com/x.png) ![gone](missing.png) [page](../index.md)\n\
             `![in code](code.png)`",
        ),
    );
    for image in [
        "docs/guide/shot.png",
        "docs/assets/logo.png",
        "docs/assets/based.png",
        "docs/guide/code.png",
        "docs/unused.png",
    ] {
        write(&root, image, "png");
    }
    write(
        &root,
        "docs/_drafts/draft.md",
        &page("Draft", "", "not a page"),
    );
    // `missing.png` is a broken link on purpose: the checks would stop the build.
    let unchecked = Options {
        no_check: true,
        ..Options::default()
    };
    let mut log = Vec::new();
    let built =
        anvil::commands::docs::build_book(&config, &root, "demo", &unchecked, &mut log).unwrap();
    let paths = archive_paths(&std::fs::read(&built.archive).unwrap());
    let sources: Vec<&str> = paths
        .iter()
        .filter_map(|p| p.strip_prefix("_source/"))
        .collect();
    assert_eq!(
        sources,
        [
            "_config.yml",
            "assets/based.png",
            "assets/logo.png",
            "guide/setup.md",
            "guide/shot.png",
            "index.md",
        ]
    );
    // The site is in the archive beside the sources, which keep their front matter.
    assert!(paths.iter().any(|p| p == "index.html"), "{paths:?}");
    let site_files = paths.iter().filter(|p| !p.starts_with("_source/")).count();
    let tar =
        quench_archive::gzip::decompress(&std::fs::read(&built.archive).unwrap(), 1 << 26).unwrap();
    let index = quench_archive::tar::Reader::new(&tar)
        .map(Result::unwrap)
        .find(|e| e.path == "_source/index.md")
        .unwrap();
    assert!(
        std::str::from_utf8(index.data)
            .unwrap()
            .starts_with("---\ntitle: Demo")
    );
    assert!(
        built.manifest_text.contains("\"sources\": \"_source/\""),
        "{}",
        built.manifest_text
    );
    assert!(
        built.manifest_text.contains("\"source_files\": 6"),
        "{}",
        built.manifest_text
    );
    assert!(
        built
            .manifest_text
            .contains(&format!("\"files\": {site_files}")),
        "the site's files only: {}",
        built.manifest_text
    );
    // Built again: the same bytes.
    let first = std::fs::read(&built.archive).unwrap();
    let again =
        anvil::commands::docs::build_book(&config, &root, "demo", &unchecked, &mut Vec::new())
            .unwrap();
    assert_eq!(first, std::fs::read(&again.archive).unwrap());
}
