use crate::support::{Scratch, settings};
use loupe::app::{Env, Outcome, run};
use loupe::cli::{Cli, LoginArgs};
use loupe::pager::{ImageState, Pager};
use loupe::released::{
    AnvilSettings, Credentials, Net, Remote, cache_root, list, open, parse_spec, remote, sources_of,
};
use quench_args::Parser;
use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Cursor;

const WAREHOUSE: &str = "https://wh.example/warehouse";

/// Warehouse as a map of URLs; records every request with the token it carried.
#[derive(Debug, Default)]
struct FakeNet {
    files: HashMap<String, (u16, Vec<u8>)>,
    token: Option<String>,
    token_error: Option<String>,
    /// When set, a request without this token is answered 401, one with another token 403.
    needs: Option<String>,
    asked: RefCell<Vec<(String, Option<String>)>>,
}

impl Net for FakeNet {
    fn get(&self, url: &str, token: Option<&str>) -> Result<(u16, Vec<u8>), String> {
        self.asked
            .borrow_mut()
            .push((url.to_string(), token.map(str::to_string)));
        if let Some(needed) = &self.needs {
            match token {
                None => return Ok((401, b"{\"error\":\"no token\"}".to_vec())),
                Some(t) if t != needed => return Ok((403, Vec::new())),
                Some(_) => {}
            }
        }
        Ok(self
            .files
            .get(url)
            .cloned()
            .unwrap_or_else(|| (404, Vec::new())))
    }

    fn token(&self) -> Result<Option<String>, String> {
        self.token_error
            .as_ref()
            .map_or_else(|| Ok(self.token.clone()), |e| Err(e.clone()))
    }
}

fn the_remote() -> Remote {
    Remote {
        warehouse: WAREHOUSE.to_string(),
        storage: "docs".to_string(),
    }
}

fn file_url(path: &str) -> String {
    the_remote().file_url(path)
}

/// A PNG of 20 × 20 pixels.
fn png() -> Vec<u8> {
    quench_image::encode_png(20, 20, &[200, 10, 10, 255].repeat(400))
}

/// A release archive: a built site, and `sources` under `_source/`.
fn archive(sources: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut tar = quench_archive::tar::Builder::new();
    tar.append_file("index.html", b"<h1>site</h1>").unwrap();
    for (path, data) in sources {
        tar.append_file(&format!("_source/{path}"), data).unwrap();
    }
    quench_archive::gzip::compress(&tar.finish())
}

fn book_sources() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("_config.yml", b"title: Forge\n".to_vec()),
        (
            "index.md",
            b"---\ntitle: Home\nnav_order: 1\n---\n# Home\n\nRead the [setup](guide/setup.md).\n"
                .to_vec(),
        ),
        (
            "guide/setup.md",
            b"---\ntitle: Setup\nnav_order: 2\n---\n# Setup\n\n![shot](shot.png)\n\nSteps.\n"
                .to_vec(),
        ),
        ("guide/shot.png", png()),
    ]
}

/// A storage with `forge` at version `1.0` (also the latest), its archive holding `sources`; `with_sources`
/// says whether the manifest says so.
fn warehouse(sources: &[(&str, Vec<u8>)], with_sources: bool) -> FakeNet {
    let bytes = archive(sources);
    let mut manifest = quench_serde::json!({
        "book": "forge",
        "title": "Forge",
        "version": "1.0",
        "archive": "forge-1.0.tar.gz",
        "archive_sha256": quench_core::hex::encode(quench_crypto::hash::sha256(&bytes)),
    });
    if with_sources {
        manifest["sources"] = "_source/".into();
    }
    let mut latest = manifest.clone();
    latest["archive_path"] = "forge/1.0/forge-1.0.tar.gz".into();
    let mut net = FakeNet::default();
    let json = |v: &quench_serde::Value| (200, quench_serde::json::to_string(v).into_bytes());
    net.files
        .insert(file_url("forge/latest.json"), json(&latest));
    net.files
        .insert(file_url("forge/1.0/manifest.json"), json(&manifest));
    net.files
        .insert(file_url("forge/1.0/forge-1.0.tar.gz"), (200, bytes));
    net
}

fn asked(net: &FakeNet) -> Vec<String> {
    net.asked
        .borrow()
        .iter()
        .map(|(url, _)| url.clone())
        .collect()
}

#[quench_test::test]
fn the_latest_release_is_fetched_into_the_cache_then_reused() {
    let cache = Scratch::new("cache");
    let net = warehouse(&book_sources(), true);
    let opened = open("forge", &the_remote(), &net, cache.path()).unwrap();
    assert_eq!(
        (opened.book.as_str(), opened.version.as_str(), opened.cached),
        ("forge", "1.0", false)
    );
    assert_eq!(opened.dir, cache.path().join("forge/1.0"));
    assert_eq!(
        asked(&net),
        [
            file_url("forge/latest.json"),
            file_url("forge/1.0/forge-1.0.tar.gz")
        ]
    );
    for file in [
        "_config.yml",
        "index.md",
        "guide/setup.md",
        "guide/shot.png",
    ] {
        assert!(opened.dir.join(file).is_file(), "{file}");
    }
    assert!(
        !opened.dir.join("index.html").exists(),
        "only _source/ is unpacked"
    );
    // Again: the latest is asked for, the archive is not downloaded again.
    net.asked.borrow_mut().clear();
    let again = open("forge", &the_remote(), &net, cache.path()).unwrap();
    assert!(again.cached);
    assert_eq!(asked(&net), [file_url("forge/latest.json")]);
    // A version given and cached: Warehouse is not asked at all.
    net.asked.borrow_mut().clear();
    let pinned = open("forge@1.0", &the_remote(), &net, cache.path()).unwrap();
    assert!(pinned.cached);
    assert_eq!(asked(&net), Vec::<String>::new());
}

#[quench_test::test]
fn a_given_version_is_fetched_by_its_manifest() {
    let cache = Scratch::new("version");
    let net = warehouse(&book_sources(), true);
    let opened = open("forge@1.0", &the_remote(), &net, cache.path()).unwrap();
    assert!(!opened.cached);
    assert_eq!(
        asked(&net),
        [
            file_url("forge/1.0/manifest.json"),
            file_url("forge/1.0/forge-1.0.tar.gz")
        ]
    );
}

#[quench_test::test]
fn a_released_book_opens_like_a_local_one() {
    let cache = Scratch::new("pager");
    let net = warehouse(&book_sources(), true);
    let opened = open("forge", &the_remote(), &net, cache.path()).unwrap();
    let book = docs_book::Book::open(&opened.dir).unwrap();
    let page = book.page("guide/setup").unwrap();
    let mut pager = Pager::book(book, page, settings(), None, (40, 30));
    pager.set_graphics(Some((10, 20)));
    assert!(
        pager.screen().status.starts_with("Forge › Setup"),
        "{}",
        pager.screen().status
    );
    let ImageState::Shown { path, .. } = &pager.images()[0].state else {
        panic!("{:?}", pager.images()[0].state);
    };
    assert!(
        path.starts_with(&opened.dir),
        "the image comes from the cache"
    );
}

/// Runs a command line against `net`, with Warehouse set and the cache in `cache`.
fn loupe(
    args: &[&str],
    net: &FakeNet,
    cwd: &Scratch,
    cache: Option<&Scratch>,
) -> (Outcome, String, String) {
    let mut words = vec!["loupe"];
    words.extend_from_slice(args);
    let cli = Cli::try_parse_from(words).unwrap();
    let env = Env {
        cwd: cwd.path().to_path_buf(),
        cache: cache.map(|c| c.path().to_path_buf()),
        warehouse_url: Some(format!("{WAREHOUSE}/")),
        ..Env::default()
    };
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let outcome = run(
        &cli,
        &env,
        net,
        &mut Cursor::new(Vec::new()),
        &mut out,
        &mut err,
    );
    (
        outcome,
        String::from_utf8(out).unwrap(),
        String::from_utf8(err).unwrap(),
    )
}

fn status(outcome: &Outcome) -> Option<i32> {
    match outcome {
        Outcome::Exit(s) => Some(*s),
        _ => None,
    }
}

#[quench_test::test]
fn book_prints_a_released_page() {
    let (cwd, cache) = (Scratch::new("cwd"), Scratch::new("printcache"));
    let net = warehouse(&book_sources(), true);
    let (outcome, out, err) = loupe(
        &["--book", "forge", "guide/setup"],
        &net,
        &cwd,
        Some(&cache),
    );
    assert_eq!(status(&outcome), Some(0), "{err}");
    assert!(out.starts_with("Setup\n═"), "{out}");
    assert!(out.contains("[image: shot]"), "{out}");
    assert!(err.starts_with("loupe: fetched forge 1.0 into "), "{err}");
    let (_, out, err) = loupe(&["--book", "forge@1.0"], &net, &cwd, Some(&cache));
    assert!(out.starts_with("Home\n"), "{out}");
    assert_eq!(err, "", "from the cache, nothing to say");
    let (outcome, _, err) = loupe(&["--book", "forge", "nowhere"], &net, &cwd, Some(&cache));
    assert_eq!(status(&outcome), Some(1));
    assert!(err.contains("no page `nowhere`"), "{err}");
}

#[quench_test::test]
fn book_on_a_terminal_is_the_pager_on_the_cache() {
    let (cwd, cache) = (Scratch::new("cwd"), Scratch::new("ttycache"));
    let net = warehouse(&book_sources(), true);
    let mut words = vec!["loupe", "--book", "forge"];
    words.push("guide/setup");
    let cli = Cli::try_parse_from(words).unwrap();
    let env = Env {
        stdout_tty: true,
        size: Some((60, 20)),
        cwd: cwd.path().to_path_buf(),
        cache: Some(cache.path().to_path_buf()),
        warehouse_url: Some(WAREHOUSE.to_string()),
        ..Env::default()
    };
    let outcome = run(
        &cli,
        &env,
        &net,
        &mut Cursor::new(Vec::new()),
        &mut Vec::new(),
        &mut Vec::new(),
    );
    let Outcome::Pager(pager, _) = outcome else {
        panic!("expected the pager")
    };
    assert!(pager.screen().status.starts_with("Forge › Setup"));
}

#[quench_test::test]
fn the_errors_say_what_to_do() {
    let (cwd, cache) = (Scratch::new("cwd"), Scratch::new("errcache"));
    let fail =
        |net: &FakeNet, spec: &str| open(spec, &the_remote(), net, cache.path()).unwrap_err();

    // Unknown book, unknown version.
    let net = warehouse(&book_sources(), true);
    assert_eq!(
        fail(&net, "quench"),
        "no released book `quench` in the `docs` storage (`loupe --book` lists them)"
    );
    assert_eq!(
        fail(&net, "forge@0.9"),
        "the book `forge` has no version `0.9` (the latest is 1.0)"
    );
    assert!(fail(&net, "../etc").contains("is not a book name"));
    assert!(fail(&net, "forge@").contains("no usable version"));

    // Credentials needed, refused, or not exchanged.
    let mut net = warehouse(&book_sources(), true);
    net.needs = Some("good".to_string());
    let error = fail(&net, "forge");
    assert!(
        error.contains("needs credentials (401): run `loupe login`"),
        "{error}"
    );
    net.token = Some("bad".to_string());
    let error = fail(&net, "forge");
    assert!(
        error.contains("refused the credentials for the `docs` storage (403)"),
        "{error}"
    );
    net.token = Some("good".to_string());
    net.asked.borrow_mut().clear();
    assert!(open("forge", &the_remote(), &net, cache.path()).is_ok());
    assert!(net.asked.borrow().iter().all(|(_, t)| t.is_some()));
    let net = FakeNet {
        token_error: Some("Gatehouse rejected the application token".to_string()),
        ..FakeNet::default()
    };
    assert!(fail(&net, "forge").starts_with("cannot get a Warehouse token: Gatehouse rejected"));

    // Released before loupe support: the manifest says no sources, or the archive has none.
    let old = Scratch::new("oldcache");
    let net = warehouse(&book_sources(), false);
    let error = open("forge", &the_remote(), &net, old.path()).unwrap_err();
    assert_eq!(
        error,
        "the release forge 1.0 has no Markdown sources (_source/): it was released before loupe support; \
         release the book again with a newer anvil"
    );
    assert_eq!(
        asked(&net),
        [file_url("forge/latest.json")],
        "the archive is not downloaded"
    );
    let net = warehouse(&[], true);
    assert!(
        open("forge", &the_remote(), &net, old.path())
            .unwrap_err()
            .contains("released before loupe support")
    );

    // A damaged archive.
    let mut net = warehouse(&book_sources(), true);
    net.files
        .get_mut(&file_url("forge/1.0/forge-1.0.tar.gz"))
        .unwrap()
        .1[30] ^= 0xff;
    assert!(
        open("forge", &the_remote(), &net, old.path())
            .unwrap_err()
            .contains("does not match its checksum")
    );
    assert!(
        !old.path().join("forge/1.0").exists(),
        "nothing half-written is left"
    );

    // No Warehouse configured.
    let cli = Cli::try_parse_from(["loupe", "--book", "forge"]).unwrap();
    let env = Env {
        cwd: cwd.path().to_path_buf(),
        cache: Some(cache.path().to_path_buf()),
        ..Env::default()
    };
    let mut err = Vec::new();
    let outcome = run(
        &cli,
        &env,
        &FakeNet::default(),
        &mut Cursor::new(Vec::new()),
        &mut Vec::new(),
        &mut err,
    );
    assert_eq!(status(&outcome), Some(1));
    let err = String::from_utf8(err).unwrap();
    assert!(
        err.starts_with("loupe: no Warehouse configured: set LOUPE_WAREHOUSE_URL"),
        "{err}"
    );
    // A ROUTE goes as the one argument.
    let net = warehouse(&book_sources(), true);
    let (outcome, _, err) = loupe(&["--book", "forge", "a", "b"], &net, &cwd, Some(&cache));
    assert_eq!(status(&outcome), Some(2));
    assert!(err.contains("the page is the one argument"), "{err}");
    let (outcome, _, err) = loupe(&["--book", "forge"], &net, &cwd, None);
    assert_eq!(status(&outcome), Some(1));
    assert!(err.contains("no cache directory"), "{err}");
}

#[quench_test::test]
fn paths_that_climb_out_of_the_sources_are_refused() {
    let mut tar = quench_archive::tar::Builder::new();
    let mut header = quench_archive::tar::Header::file("_source/../../evil.md");
    header.mtime = 0;
    tar.append(&header, b"x").unwrap();
    let bytes = quench_archive::gzip::compress(&tar.finish());
    assert!(sources_of(&bytes).unwrap_err().contains("climbs out"));
    assert!(sources_of(b"not gzip").is_err());
}

#[quench_test::test]
fn book_alone_lists_the_released_books() {
    let cwd = Scratch::new("cwd");
    let mut net = warehouse(&book_sources(), true);
    let listing = quench_serde::json!({
        "storage": "docs",
        "prefix": "",
        "entries": [
            {"name": "forge", "path": "forge", "kind": "directory"},
            {"name": "stray", "path": "stray", "kind": "directory"},
            {"name": ".trash", "path": ".trash", "kind": "directory"},
            {"name": "latest.json", "path": "quench/latest.json", "kind": "file"},
            {"name": "notes.txt", "path": "notes.txt", "kind": "file"},
        ]
    });
    net.files.insert(
        the_remote().list_url(),
        (200, quench_serde::json::to_string(&listing).into_bytes()),
    );
    net.files.insert(
        file_url("quench/latest.json"),
        (
            200,
            br#"{"book": "quench", "version": "2026.11.01", "title": "Quench"}"#.to_vec(),
        ),
    );
    let books = list(&the_remote(), &net).unwrap();
    let names: Vec<&str> = books.iter().map(|b| b.book.as_str()).collect();
    assert_eq!(names, ["forge", "quench"], "`stray` has no latest.json");
    let (outcome, out, _) = loupe(&["--book"], &net, &cwd, None);
    assert_eq!(status(&outcome), Some(0));
    assert_eq!(
        out,
        "forge   1.0         Forge\nquench  2026.11.01  Quench\n"
    );
    let empty = FakeNet {
        files: HashMap::from([(
            the_remote().list_url(),
            (200, br#"{"entries": []}"#.to_vec()),
        )]),
        ..FakeNet::default()
    };
    let (_, out, _) = loupe(&["--book"], &empty, &cwd, None);
    assert_eq!(
        out,
        format!("no released books in the `docs` storage at {WAREHOUSE}\n")
    );
    let (outcome, _, err) = loupe(&["--book"], &FakeNet::default(), &cwd, None);
    assert_eq!(status(&outcome), Some(1));
    assert!(err.contains("no files storage `docs` (404)"), "{err}");
}

#[quench_test::test]
fn warehouse_and_storage_come_from_the_environment_the_config_or_anvil() {
    let dir = Scratch::new("anvil");
    dir.write(
        ".anvil.toml",
        "[artifacts]\nwarehouse_url = \"https://artifacts.example/warehouse\"\n[release]\ngatehouse_url = \"https://gh.example/gatehouse\"\n",
    );
    dir.write(
        ".anvil/docs.toml",
        "[docs]\nstorage = \"books\"\nwarehouse_url = \"https://docs.example/warehouse/\"\n",
    );
    dir.write(".anvil/secret.toml", "app_token = \"not settings\"\n");
    let nested = dir.path().join("sub/dir");
    std::fs::create_dir_all(&nested).unwrap();
    let anvil = AnvilSettings::find(&nested);
    assert_eq!(
        anvil,
        AnvilSettings {
            warehouse_url: Some("https://docs.example/warehouse/".to_string()),
            storage: Some("books".to_string()),
            gatehouse_url: Some("https://gh.example/gatehouse".to_string()),
        }
    );
    let from_anvil = remote(None, None, None, None, &anvil).unwrap();
    assert_eq!(
        (from_anvil.warehouse.as_str(), from_anvil.storage.as_str()),
        ("https://docs.example/warehouse", "books")
    );
    let from_config = remote(
        None,
        None,
        Some("https://config.example/w"),
        Some("mine"),
        &anvil,
    )
    .unwrap();
    assert_eq!(
        (from_config.warehouse.as_str(), from_config.storage.as_str()),
        ("https://config.example/w", "mine")
    );
    let from_env = remote(
        Some("https://env.example/w"),
        Some(" "),
        Some("https://config.example/w"),
        None,
        &anvil,
    )
    .unwrap();
    assert_eq!(
        (from_env.warehouse.as_str(), from_env.storage.as_str()),
        ("https://env.example/w", "books")
    );
    let none = remote(None, None, None, None, &AnvilSettings::default()).unwrap_err();
    assert!(none.starts_with("no Warehouse configured"), "{none}");
    assert_eq!(
        remote(
            Some("https://w"),
            None,
            None,
            None,
            &AnvilSettings::default()
        )
        .unwrap()
        .storage,
        "docs"
    );
    assert_eq!(
        the_remote().file_url("forge/1.0/a b.tar.gz"),
        "https://wh.example/warehouse/api/v1/files/docs/file?path=forge/1.0/a%20b.tar.gz"
    );
}

#[quench_test::test]
fn specs_and_the_cache_directory() {
    assert_eq!(parse_spec("forge").unwrap(), ("forge".to_string(), None));
    assert_eq!(
        parse_spec("forge@2026.11.08").unwrap(),
        ("forge".to_string(), Some("2026.11.08".to_string()))
    );
    assert!(parse_spec("").is_err());
    assert!(parse_spec("a/b").is_err());
    assert!(parse_spec("forge@..").is_err());
    assert_eq!(
        cache_root(Some("/xdg"), Some("/home/me")),
        Some("/xdg/loupe".into())
    );
    assert_eq!(
        cache_root(None, Some("/home/me")),
        Some("/home/me/.cache/loupe".into())
    );
    assert_eq!(cache_root(Some("relative"), None), None);
}

#[quench_test::test]
fn stored_credentials_are_looked_for_in_order() {
    let cwd = Scratch::new("creds");
    let config = Scratch::new("config");
    let credentials = |anvil_app: Option<(String, String)>| Credentials {
        cwd: cwd.path().join("deep"),
        config_dir: Some(config.path().to_path_buf()),
        token: None,
        anvil_app,
    };
    std::fs::create_dir_all(cwd.path().join("deep")).unwrap();
    assert_eq!(credentials(None).stored().unwrap(), None);
    cwd.write(
        ".anvil.secret.toml",
        "gatehouse_url = \"https://anvil\"\napp_token = \"a\"\n",
    );
    assert_eq!(credentials(None).stored().unwrap().unwrap().app_token, "a");
    let ci = Some(("ci".to_string(), "https://ci".to_string()));
    assert_eq!(
        credentials(ci.clone()).stored().unwrap().unwrap().app_token,
        "ci"
    );
    config.write(
        ".loupe.secret.toml",
        "gatehouse_url = \"https://config\"\napp_token = \"c\"\n",
    );
    assert_eq!(
        credentials(ci.clone()).stored().unwrap().unwrap().app_token,
        "c"
    );
    cwd.write(
        ".loupe.secret.toml",
        "gatehouse_url = \"https://here\"\napp_token = \"h\"\n",
    );
    assert_eq!(credentials(ci).stored().unwrap().unwrap().app_token, "h");
}

#[quench_test::test]
fn login_asks_for_a_read_token_by_default() {
    let token = loupe::login::new_token(&LoginArgs::default(), "box");
    assert_eq!(
        (token.name.as_str(), token.scope.as_slice()),
        ("loupe on box", &["warehouse:read".to_string()][..])
    );
    let args = LoginArgs {
        scope: vec!["warehouse:write".to_string()],
        name: Some("mine".to_string()),
        days: Some(30),
        ..LoginArgs::default()
    };
    let token = loupe::login::new_token(&args, "box");
    assert_eq!(
        (token.name.as_str(), token.expires_in_days),
        ("mine", Some(30))
    );
    assert_eq!(token.scope, ["warehouse:write"]);
}

#[quench_test::test]
fn login_takes_its_gatehouse_url_from_the_flag_the_environment_the_config_or_anvil() {
    let cwd = Scratch::new("loginurl");
    cwd.write(
        ".anvil.toml",
        "[release]\ngatehouse_url = \"https://anvil/gatehouse\"\n",
    );
    let run_login = |args: &[&str], env_url: Option<&str>, config: Option<&str>| {
        let mut words = vec!["loupe", "login"];
        words.extend_from_slice(args);
        let cli = Cli::try_parse_from(words).unwrap();
        let env = Env {
            cwd: cwd.path().to_path_buf(),
            gatehouse_url: env_url.map(str::to_string),
            config: config.map(|c| ("/c.toml".into(), format!("gatehouse_url = \"{c}\"\n"))),
            ..Env::default()
        };
        match run(
            &cli,
            &env,
            &FakeNet::default(),
            &mut Cursor::new(Vec::new()),
            &mut Vec::new(),
            &mut Vec::new(),
        ) {
            Outcome::Login(_, url) => url,
            other => panic!("{other:?}"),
        }
    };
    assert_eq!(
        run_login(&[], None, None).as_deref(),
        Some("https://anvil/gatehouse")
    );
    assert_eq!(
        run_login(&[], None, Some("https://config")).as_deref(),
        Some("https://config")
    );
    assert_eq!(
        run_login(&[], Some("https://env"), Some("https://config")).as_deref(),
        Some("https://env")
    );
    assert_eq!(
        run_login(
            &[
                "--gatehouse-url",
                "https://flag",
                "--scope",
                "a:b",
                "--scope",
                "c:d"
            ],
            Some("https://env"),
            None
        )
        .as_deref(),
        Some("https://flag")
    );
}

/// The real HTTP client against an in-process server: the token goes as a bearer, statuses come back as they are.
#[quench_test::test(flavor = "multi_thread", worker_threads = 2)]
async fn http_sends_the_bearer_token() {
    use quench_test::mock::matchers::{header, method, path, query_param};
    use quench_test::mock::{Mock, MockServer, ResponseTemplate};
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/warehouse/api/v1/files/docs/file"))
        .and(query_param("path", "forge/latest.json"))
        .and(header("authorization", "Bearer tok"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw("{\"version\": \"1.0\"}", "application/json"),
        )
        .mount(&server)
        .await;
    let base = format!("{}/warehouse", server.uri());
    let (ok, missing, token) = tokio::task::spawn_blocking(move || {
        let http = loupe::released::Http::new(Credentials {
            cwd: std::env::temp_dir().join("loupe-no-such-dir"),
            config_dir: None,
            token: Some("tok".to_string()),
            anvil_app: None,
        })
        .unwrap();
        let remote = Remote {
            warehouse: base,
            storage: "docs".to_string(),
        };
        let ok = http
            .get(&remote.file_url("forge/latest.json"), Some("tok"))
            .unwrap();
        let missing = http
            .get(&remote.file_url("quench/latest.json"), Some("tok"))
            .unwrap();
        (ok, missing.0, http.token().unwrap())
    })
    .await
    .unwrap();
    assert_eq!(ok, (200, b"{\"version\": \"1.0\"}".to_vec()));
    assert_eq!(missing, 404);
    assert_eq!(token.as_deref(), Some("tok"));
}
