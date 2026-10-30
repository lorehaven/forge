use std::cell::RefCell;
use std::path::{Path, PathBuf};

use anvil::commands::artifact::Upload;
use anvil::commands::docs::{
    Options, check_book, destinations, links, normalise_route, openapi_problems, release_with,
    rewrite_links, source_routes, split_front_matter,
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

/// A stand-in for Jekyll: answers `--version`, and `build` writes a small site into `--destination`.
#[cfg(unix)]
fn fake_jekyll(root: &Path) -> String {
    use std::os::unix::fs::PermissionsExt as _;
    let script = root.join("fake-jekyll");
    std::fs::write(
        &script,
        "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo jekyll 4; exit 0; fi\n\
         while [ $# -gt 0 ]; do case \"$1\" in --destination) dest=\"$2\"; shift;; esac; shift; done\n\
         mkdir -p \"$dest/sub\" && echo '<h1>hi</h1>' > \"$dest/index.html\" && echo x > \"$dest/sub/a.html\"\n",
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    script.to_string_lossy().into_owned()
}

#[cfg(unix)]
fn demo(label: &str) -> (PathBuf, Config) {
    let root = temp(label);
    write(
        &root,
        "docs/index.md",
        &page("Demo", "nav_order: 1\n", "hello"),
    );
    let jekyll = fake_jekyll(&root);
    let config = config(BOOK);
    let config = Config {
        docs: anvil::config::DocsConfig {
            jekyll: Some(jekyll),
            ..config.docs
        },
        ..config
    };
    (root, config)
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
        manifest.contains("\"book\": \"demo\"") && manifest.contains("\"files\": 2"),
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
fn links_between_pages_point_at_the_html_jekyll_writes() {
    let body = "[a](a.md) [b](../dir/b.md#part) [c](c.md?x=1) [d](https://x.example/d.md) [e](dir/) [f](f.pdf)\n\
                `[no](code.md)`\n```\n[no](fenced.md)\n```\n[g](G.MD)\n";
    assert_eq!(
        rewrite_links(body),
        "[a](a.html) [b](../dir/b.html#part) [c](c.html?x=1) [d](https://x.example/d.md) [e](dir/) [f](f.pdf)\n\
         `[no](code.md)`\n```\n[no](fenced.md)\n```\n[g](G.html)\n"
    );
}

#[cfg(unix)]
#[quench_test::test]
fn jekyll_is_given_a_staged_copy_with_the_links_rewritten() {
    use std::os::unix::fs::PermissionsExt as _;
    let (root, config) = demo("staged");
    write(
        &root,
        "docs/other.md",
        &page("Other", "nav_order: 2\n", "[home](index.md)"),
    );
    // A Jekyll that copies what it is given, so the test can look at it.
    let script = root.join("copy-jekyll");
    std::fs::write(
        &script,
        "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then exit 0; fi\n\
         while [ $# -gt 0 ]; do case \"$1\" in --destination) dest=\"$2\"; shift;; --source) src=\"$2\"; shift;; esac; shift; done\n\
         mkdir -p \"$dest\" && cp -r \"$src\"/. \"$dest\"/\n",
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let config = Config {
        docs: anvil::config::DocsConfig {
            jekyll: Some(script.to_string_lossy().into_owned()),
            ..config.docs
        },
        ..config
    };
    anvil::commands::docs::build(
        &config,
        &root,
        Some("demo"),
        &Options::default(),
        &mut Vec::new(),
    )
    .unwrap();
    let built = std::fs::read_to_string(root.join("target/docs/demo/site/other.md")).unwrap();
    assert!(built.contains("[home](index.html)"), "{built}");
    // The book itself is untouched.
    let original = std::fs::read_to_string(root.join("docs/other.md")).unwrap();
    assert!(original.contains("[home](index.md)"), "{original}");
}

#[cfg(unix)]
#[quench_test::test]
fn a_missing_gem_is_named_in_the_error() {
    use std::os::unix::fs::PermissionsExt as _;
    let (root, config) = demo("gem");
    let script = root.join("bad-jekyll");
    std::fs::write(
        &script,
        "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then exit 0; fi\n\
         echo \"cannot load such file -- jekyll-remote-theme (LoadError)\" >&2\nexit 1\n",
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let config = Config {
        docs: anvil::config::DocsConfig {
            jekyll: Some(script.to_string_lossy().into_owned()),
            ..config.docs
        },
        ..config
    };
    let err =
        anvil::commands::docs::build(&config, &root, None, &Options::default(), &mut Vec::new())
            .unwrap_err()
            .to_string();
    assert!(err.contains("gem install jekyll-remote-theme"), "{err}");
}
