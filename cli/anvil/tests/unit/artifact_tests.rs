use std::cell::RefCell;
use std::path::{Path, PathBuf};

use anvil::commands::artifact::{
    Options, Upload, Version, decide, expand, list_url, parse_published, read_version,
    release_all_with, release_managed, release_with, set_version, upload_url, version_code,
};
use anvil::config::Config;

fn temp(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("anvil-artifact-{}-{label}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A project with a version and the two builds hoist has: an APK and a `.deb`, each made by a tiny shell command.
fn project(label: &str, version: &str) -> (PathBuf, Config) {
    let dir = temp(label);
    std::fs::write(
        dir.join("Cargo.toml"),
        format!("[workspace]\nmembers = []\n\n[workspace.package]\nversion = \"{version}\"\n"),
    )
    .unwrap();
    let config: Config = quench_serde::toml::from_str(
        r#"
[artifacts]
warehouse_url = "https://warehouse.example/warehouse/"

[artifacts.programs.hoist]
program = "dev.lorehaven.hoist"
version_file = "Cargo.toml"

[[artifacts.programs.hoist.targets]]
platform = "android"
format = "apk"
build = ["sh", "-c", "mkdir -p dist && printf 'code=%s' $ANVIL_VERSION_CODE > dist/hoist-${version_name}-${version_code}.apk"]
file = "dist/hoist-${version_name}-${version_code}.apk"

[[artifacts.programs.hoist.targets]]
name = "linux-deb"
platform = "linux"
format = "deb"
arch = "x86_64"
label = "Hoist App"
build = ["sh", "-c", "mkdir -p dist && printf deb > dist/hoist_${version_name}_amd64.deb"]
file = "dist/hoist_${version_name}_amd64.deb"
"#,
    )
    .unwrap();
    (dir, config)
}

type Sent = RefCell<Vec<Upload>>;

fn run(
    config: &Config,
    root: &Path,
    options: &Options,
    status: u16,
) -> (Result<(), String>, String, Vec<Upload>, usize) {
    let sent: Sent = RefCell::default();
    let asked = RefCell::new(0_usize);
    let mut out = Vec::new();
    let result = release_with(
        config,
        root,
        "hoist",
        options,
        &mut out,
        &|| {
            *asked.borrow_mut() += 1;
            Ok("short-lived".to_string())
        },
        &|upload| {
            sent.borrow_mut().push(upload.clone());
            Ok((status, "reply".to_string()))
        },
    )
    .map_err(|e| e.to_string());
    (
        result,
        String::from_utf8(out).unwrap(),
        sent.into_inner(),
        asked.into_inner(),
    )
}

#[quench_test::test]
fn a_version_is_one_number_the_same_way_quench_makes_it() {
    // The same table as quench-tool's own test: the APK and the desktop builds of a version must agree.
    for (version, code) in [
        ("0.1.0", 1_000),
        ("0.1.3", 1_003),
        ("1.2.3", 1_002_003),
        ("12.0.7", 12_000_007),
        ("1.2.3-rc.1", 1_002_003),
        ("2", 2_000_000),
        ("1.5000.0", 1_999_000),
        ("nonsense", 0),
    ] {
        assert_eq!(version_code(version), code, "{version}");
    }
}

#[quench_test::test]
fn the_version_is_the_workspaces_else_the_packages_and_a_missing_one_is_said() {
    let dir = temp("version");
    let write = |text: &str| std::fs::write(dir.join("Cargo.toml"), text).unwrap();
    write("[workspace.package]\nversion = \"1.2.3\"\n[package]\nversion = \"9.9.9\"\n");
    assert_eq!(read_version(&dir.join("Cargo.toml")).unwrap(), "1.2.3");
    write("[package]\nname = \"x\"\nversion = \"4.5.6\"\n");
    assert_eq!(read_version(&dir.join("Cargo.toml")).unwrap(), "4.5.6");
    write("[package]\nname = \"x\"\n");
    assert!(
        read_version(&dir.join("Cargo.toml"))
            .unwrap_err()
            .to_string()
            .contains("no version")
    );
    assert!(read_version(&dir.join("missing.toml")).is_err());
}

#[quench_test::test]
fn an_address_says_what_the_warehouse_cannot_read_from_the_file() {
    let (_, config) = project("url", "0.1.3");
    let program = &config.artifacts.programs["hoist"];
    // Android: the APK carries its own identity, so there is no format to say.
    let apk = upload_url(
        "https://w.example/warehouse/",
        &program.program,
        &program.targets[0],
        1_003,
        "0.1.3",
        "hoist.apk",
    );
    assert_eq!(
        apk,
        "https://w.example/warehouse/api/v1/artifacts/dev.lorehaven.hoist/android/1003?format=apk&version_name=0.1.3&filename=hoist.apk"
    );
    let deb = upload_url(
        "https://w.example/warehouse",
        &program.program,
        &program.targets[1],
        1_003,
        "0.1.3",
        "hoist_0.1.3_amd64.deb",
    );
    assert!(
        deb.contains("/linux/1003?format=deb&arch=x86_64&version_name=0.1.3&label=Hoist%20App&filename=hoist_0.1.3_amd64.deb"),
        "{deb}"
    );
    assert_eq!(
        expand(
            "${program}-${version_name}-${version_code}",
            "p",
            "1.0.0",
            1_000_000
        ),
        "p-1.0.0-1000000"
    );
}

#[quench_test::test]
fn every_target_is_built_with_the_one_version_and_sent_with_a_short_lived_token() {
    let (dir, config) = project("release", "0.2.5");
    let (result, out, sent, asked) = run(&config, &dir, &Options::default(), 201);
    result.unwrap();
    assert_eq!(asked, 1, "one token for the whole release");
    assert_eq!(sent.len(), 2);
    assert!(sent[0].url.starts_with(
        "https://warehouse.example/warehouse/api/v1/artifacts/dev.lorehaven.hoist/android/2005?"
    ));
    assert_eq!(
        (sent[0].token.as_str(), sent[0].content_type.as_str()),
        ("short-lived", "application/vnd.android.package-archive")
    );
    // The build saw the number in its environment and in its arguments.
    assert_eq!(sent[0].body, b"code=2005");
    assert!(sent[1].url.contains("/linux/2005?format=deb&arch=x86_64"));
    assert_eq!(
        (sent[1].body.as_slice(), sent[1].content_type.as_str()),
        (b"deb".as_slice(), "application/octet-stream")
    );
    assert!(
        out.contains("version_code 2005") && out.matches("published").count() >= 2,
        "{out}"
    );
}

#[quench_test::test]
fn one_target_can_be_chosen_and_a_dry_run_builds_but_sends_nothing() {
    let (dir, config) = project("choose", "0.1.0");
    let only = Options {
        target: Some("linux-deb".into()),
        ..Options::default()
    };
    let (result, _, sent, _) = run(&config, &dir, &only, 201);
    result.unwrap();
    assert_eq!(sent.len(), 1);
    assert!(sent[0].url.contains("/linux/1000"));

    let dry = Options {
        dry_run: true,
        ..Options::default()
    };
    let (result, out, sent, asked) = run(&config, &dir, &dry, 201);
    result.unwrap();
    assert!(
        sent.is_empty() && asked == 0,
        "nothing sent, no login asked for"
    );
    assert!(
        out.contains("would publish") && out.contains("PUT https://"),
        "{out}"
    );
    assert!(
        dir.join("dist/hoist-0.1.0-1000.apk").is_file(),
        "it did build"
    );

    let (result, ..) = run(
        &config,
        &dir,
        &Options {
            target: Some("windows".into()),
            ..Options::default()
        },
        201,
    );
    let why = result.unwrap_err();
    assert!(
        why.contains("no target `windows`") && why.contains("android, linux-deb"),
        "{why}"
    );
}

#[quench_test::test]
fn what_the_warehouse_refuses_is_explained() {
    let (dir, config) = project("refused", "0.1.0");
    let (result, ..) = run(&config, &dir, &Options::default(), 409);
    let why = result.unwrap_err();
    assert!(
        why.contains("already published") && why.contains("immutable"),
        "{why}"
    );
    // To finish a release that stopped half way, what exists is left alone.
    let (result, out, sent, _) = run(
        &config,
        &dir,
        &Options {
            skip_existing: true,
            ..Options::default()
        },
        409,
    );
    result.unwrap();
    assert_eq!(sent.len(), 2);
    assert!(out.contains("already published, skipped"), "{out}");
    let (result, ..) = run(&config, &dir, &Options::default(), 401);
    assert!(
        result
            .unwrap_err()
            .contains("anvil login --scope warehouse:write")
    );
    let (result, ..) = run(&config, &dir, &Options::default(), 500);
    assert!(result.unwrap_err().contains("HTTP 500 reply"));
}

#[quench_test::test]
fn mistakes_in_the_setup_are_named_before_anything_is_sent() {
    let (dir, mut config) = project("mistakes", "0.1.0");
    let (result, ..) = {
        let mut out = Vec::new();
        let r = release_with(
            &config,
            &dir,
            "nope",
            &Options::default(),
            &mut out,
            &|| Ok(String::new()),
            &|_| Ok((201, String::new())),
        );
        (r.map_err(|e| e.to_string()), out)
    };
    let why = result.unwrap_err();
    assert!(
        why.contains("no artifact `nope`") && why.contains("known: hoist"),
        "{why}"
    );
    // A build that fails, and one that leaves no file.
    config.artifacts.programs.get_mut("hoist").unwrap().targets[0].build = vec!["false".into()];
    let (result, _, sent, _) = run(&config, &dir, &Options::default(), 201);
    assert!(result.unwrap_err().contains("failed"));
    assert!(sent.is_empty());
    config.artifacts.programs.get_mut("hoist").unwrap().targets[0].build = vec!["true".into()];
    config.artifacts.programs.get_mut("hoist").unwrap().targets[0].file = "dist/missing.apk".into();
    let (result, ..) = run(&config, &dir, &Options::default(), 201);
    assert!(result.unwrap_err().contains("the build left no file"));
    config.artifacts.warehouse_url = None;
    let (result, ..) = run(&config, &dir, &Options::default(), 201);
    assert!(result.unwrap_err().contains("no warehouse"));
}

#[quench_test::test]
fn a_tree_with_uncommitted_changes_is_not_released_without_saying_so() {
    let (dir, config) = project("dirty", "0.1.0");
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .arg("-C")
            .arg(&dir)
            .args(args)
            .output()
            .unwrap();
    };
    git(&["init", "-q"]);
    let (result, _, sent, _) = run(&config, &dir, &Options::default(), 201);
    let why = result.unwrap_err();
    assert!(
        why.contains("uncommitted changes") && sent.is_empty(),
        "{why}"
    );
    let (result, out, sent, _) = run(
        &config,
        &dir,
        &Options {
            allow_dirty: true,
            ..Options::default()
        },
        201,
    );
    result.unwrap();
    assert_eq!(sent.len(), 2, "{out}");
}

#[quench_test::test]
fn a_file_goes_over_the_wire_as_one_put_with_a_bearer_token() {
    use std::io::{BufRead, BufReader, Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut request_line = String::new();
        reader.read_line(&mut request_line).unwrap();
        let (mut length, mut auth, mut kind) = (0_usize, String::new(), String::new());
        loop {
            let mut header = String::new();
            reader.read_line(&mut header).unwrap();
            if header.trim().is_empty() {
                break;
            }
            let lower = header.to_ascii_lowercase();
            if let Some(v) = lower.strip_prefix("content-length:") {
                length = v.trim().parse().unwrap();
            } else if lower.starts_with("authorization:") {
                auth = header.split_once(':').unwrap().1.trim().to_string();
            } else if lower.starts_with("content-type:") {
                kind = header.split_once(':').unwrap().1.trim().to_string();
            }
        }
        let mut body = vec![0_u8; length];
        reader.read_exact(&mut body).unwrap();
        stream
            .write_all(b"HTTP/1.1 201 Created\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .unwrap();
        (request_line, auth, kind, body)
    });
    let upload = Upload {
        url: format!("http://{addr}/api/v1/artifacts/p/linux/1?format=deb"),
        token: "tok".into(),
        content_type: "application/octet-stream".into(),
        body: b"the bytes".to_vec(),
    };
    assert_eq!(anvil::commands::artifact::send(&upload).unwrap().0, 201);
    let (line, auth, kind, body) = server.join().unwrap();
    assert!(
        line.starts_with("PUT /api/v1/artifacts/p/linux/1?format=deb "),
        "{line}"
    );
    assert_eq!(
        (auth.as_str(), kind.as_str()),
        ("Bearer tok", "application/octet-stream")
    );
    assert_eq!(body, b"the bytes");
    // Nothing listening: said, not a panic.
    let nowhere = Upload {
        url: "http://127.0.0.1:1/x".into(),
        ..upload
    };
    assert!(
        anvil::commands::artifact::send(&nowhere)
            .unwrap_err()
            .to_string()
            .contains("could not be reached")
    );
}

#[quench_test::test]
fn a_service_image_is_released_with_the_same_version_or_alone() {
    let (dir, mut config) = project("artifact-image", "0.2.0");
    std::fs::write(dir.join("Dockerfile"), "FROM scratch\n").unwrap();
    config.artifacts.programs.get_mut("hoist").unwrap().image =
        Some(anvil::config::ArtifactImage {
            dockerfile: "Dockerfile".into(),
            module: "hoist".into(),
            name: "hoist".into(),
            registries: vec!["a.example/forge".into(), "b.example/forge".into()],
            ..Default::default()
        });
    let dry = Options {
        dry_run: true,
        ..Options::default()
    };
    let (result, out, ..) = run(&config, &dir, &dry, 201);
    result.unwrap();
    assert!(
        out.contains("would publish")
            && out.contains("[image] would build")
            && out.contains("a.example/forge/hoist/hoist:0.2.0, b.example/forge/hoist/hoist:0.2.0"),
        "{out}"
    );

    let alone = Options {
        dry_run: true,
        target: Some("image".into()),
        ..Options::default()
    };
    let (result, out, ..) = run(&config, &dir, &alone, 201);
    result.unwrap();
    assert!(
        !out.contains("would publish") && out.contains("[image]"),
        "{out}"
    );

    let (result, out, ..) = run(
        &config,
        &dir,
        &Options {
            dry_run: true,
            target: Some("linux-deb".into()),
            ..Options::default()
        },
        201,
    );
    result.unwrap();
    assert!(
        !out.contains("[image]"),
        "a named target leaves the image alone: {out}"
    );
}

// ---- version management and --all

const BOTH: &str = r#"[{"program":"dev.lorehaven.hoist","platform":"android","format":"apk","version_code":1003,"yanked":false},
{"program":"dev.lorehaven.hoist","platform":"linux","format":"deb","version_code":1003,"yanked":true},
{"program":"dev.lorehaven.hoist","platform":"android","format":"apk","version_code":1002}]"#;
const ANDROID_ONLY: &str = r#"[{"platform":"android","format":"apk","version_code":1003}]"#;

/// A git repository with the project committed, so a raised version can be committed too.
fn repository(label: &str, version: &str) -> (PathBuf, Config) {
    let (dir, config) = project(label, version);
    let git = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(&dir)
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    };
    git(&["init", "-q"]);
    git(&["config", "user.name", "anvil test"]);
    git(&["config", "user.email", "anvil@test.invalid"]);
    git(&["config", "commit.gpgsign", "false"]);
    std::fs::write(dir.join(".gitignore"), "dist/\n").unwrap();
    git(&["add", "-A"]);
    git(&["commit", "-q", "-m", "init"]);
    (dir, config)
}

fn git_out(dir: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// Runs `release_managed` against a stand-in warehouse holding `listed`; returns the result, the output, what was sent
/// and how many times the warehouse was asked what it holds.
fn managed(
    config: &Config,
    root: &Path,
    options: &Options,
    listed: (u16, &str),
) -> (Result<(), String>, String, Vec<Upload>, usize) {
    let sent: Sent = RefCell::default();
    let asked = RefCell::new(0_usize);
    let mut out = Vec::new();
    let result = release_managed(
        config,
        root,
        "hoist",
        options,
        &mut out,
        &|| Ok("short-lived".to_string()),
        &|upload| {
            sent.borrow_mut().push(upload.clone());
            Ok((201, String::new()))
        },
        &|url, token| {
            assert_eq!(
                url,
                "https://warehouse.example/warehouse/api/v1/artifacts/dev.lorehaven.hoist"
            );
            assert_eq!(token, "short-lived");
            *asked.borrow_mut() += 1;
            Ok((listed.0, listed.1.to_string()))
        },
    )
    .map_err(|e| e.to_string());
    (
        result,
        String::from_utf8(out).unwrap(),
        sent.into_inner(),
        asked.into_inner(),
    )
}

fn bump() -> Options {
    Options {
        bump: true,
        ..Options::default()
    }
}

#[quench_test::test]
fn the_warehouses_list_is_read_and_a_wrong_answer_is_said() {
    let published = parse_published(BOTH).unwrap();
    assert_eq!(published.len(), 3);
    assert!(
        published.contains(&("linux".to_string(), "deb".to_string(), 1003)),
        "yanked ones count: they still hold the version"
    );
    assert!(parse_published("[]").unwrap().is_empty());
    assert!(parse_published("not json").is_err());
    assert!(parse_published("{}").is_err());
    assert_eq!(
        list_url("https://w.example/warehouse/", "dev.lorehaven.hoist"),
        "https://w.example/warehouse/api/v1/artifacts/dev.lorehaven.hoist"
    );
}

#[quench_test::test]
fn the_version_is_kept_finished_or_raised_by_what_the_warehouse_holds() {
    let (_, config) = project("decide", "0.1.3");
    let program = &config.artifacts.programs["hoist"];
    let held = |list: &[(&str, &str, u32)]| -> Vec<(String, String, u32)> {
        list.iter()
            .map(|(p, f, c)| ((*p).into(), (*f).into(), *c))
            .collect()
    };
    assert_eq!(
        decide(program, "0.1.3", &held(&[]), None).unwrap(),
        Version::Keep
    );
    assert_eq!(
        decide(
            program,
            "0.1.3",
            &held(&[("android", "apk", 1002), ("linux", "deb", 1002)]),
            None
        )
        .unwrap(),
        Version::Keep,
        "older versions say nothing about this one"
    );
    assert_eq!(
        decide(program, "0.1.3", &held(&[("android", "apk", 1003)]), None).unwrap(),
        Version::Finish
    );
    let bumped = |from: &str, to: &str| Version::Bump {
        from: from.into(),
        to: to.into(),
    };
    assert_eq!(
        decide(
            program,
            "0.1.3",
            &held(&[("android", "apk", 1003), ("linux", "deb", 1003)]),
            None
        )
        .unwrap(),
        bumped("0.1.3", "0.1.4")
    );
    assert_eq!(
        decide(
            program,
            "0.1.9",
            &held(&[("android", "apk", 1009), ("linux", "deb", 1009)]),
            None
        )
        .unwrap(),
        bumped("0.1.9", "0.1.10")
    );
    // A format that is not the one the target ships does not count.
    assert_eq!(
        decide(
            program,
            "0.1.3",
            &held(&[("android", "apk", 1003), ("linux", "rpm", 1003)]),
            None
        )
        .unwrap(),
        Version::Finish
    );
    // Asking for one target: only that one decides.
    assert_eq!(
        decide(
            program,
            "0.1.3",
            &held(&[("android", "apk", 1003)]),
            Some("android")
        )
        .unwrap(),
        bumped("0.1.3", "0.1.4")
    );
    assert!(
        decide(
            program,
            "nightly",
            &held(&[("android", "apk", 0), ("linux", "deb", 0)]),
            None
        )
        .is_err()
    );
}

#[quench_test::test]
fn a_version_is_raised_in_the_file_it_lives_in() {
    let dir = temp("set-version");
    let file = dir.join("Cargo.toml");
    std::fs::write(
        &file,
        "[package]\nname = \"x\"\nversion = \"0.1.3\" # the app\n[dependencies]\nversion = \"9\"\n",
    )
    .unwrap();
    set_version(&file, "0.1.4").unwrap();
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "[package]\nname = \"x\"\nversion = \"0.1.4\" # the app\n[dependencies]\nversion = \"9\"\n"
    );
    std::fs::write(&file, "[workspace.package]\nversion = \"1.2.3\"\n").unwrap();
    set_version(&file, "1.2.4").unwrap();
    assert_eq!(read_version(&file).unwrap(), "1.2.4");
    std::fs::write(&file, "[package]\nname = \"x\"\n").unwrap();
    assert!(set_version(&file, "0.0.1").is_err());
}

#[quench_test::test]
fn without_bump_the_warehouse_is_not_asked_and_nothing_changes() {
    let (dir, config) = project("no-bump", "0.1.3");
    let (result, out, sent, asked) = managed(&config, &dir, &Options::default(), (200, BOTH));
    result.unwrap();
    assert_eq!((sent.len(), asked), (2, 0), "{out}");
}

#[quench_test::test]
fn a_version_the_warehouse_does_not_have_is_released_as_it_is() {
    let (dir, config) = repository("keep", "0.1.3");
    let (result, out, sent, asked) = managed(&config, &dir, &bump(), (200, "[]"));
    result.unwrap();
    assert_eq!((sent.len(), asked), (2, 1), "{out}");
    assert!(sent[0].url.contains("/android/1003?"), "{}", sent[0].url);
    assert_eq!(read_version(&dir.join("Cargo.toml")).unwrap(), "0.1.3");
    assert_eq!(
        git_out(&dir, &["log", "--format=%s"]),
        "init",
        "no commit for a version that was not raised"
    );
}

#[quench_test::test]
fn a_version_the_warehouse_has_is_raised_committed_and_released_as_the_new_one() {
    let (dir, config) = repository("raise", "0.1.3");
    let (result, out, sent, asked) = managed(&config, &dir, &bump(), (200, BOTH));
    result.unwrap();
    assert_eq!(asked, 1);
    assert_eq!(sent.len(), 2, "{out}");
    assert!(
        sent[0].url.contains("/android/1004?") && sent[0].url.contains("version_name=0.1.4"),
        "{}",
        sent[0].url
    );
    assert!(sent[1].url.contains("/linux/1004?"), "{}", sent[1].url);
    assert!(out.contains("raised to 0.1.4 and committed"), "{out}");
    assert_eq!(read_version(&dir.join("Cargo.toml")).unwrap(), "0.1.4");
    assert_eq!(
        git_out(&dir, &["log", "-1", "--format=%s"]),
        "release: hoist 0.1.4"
    );
    assert_eq!(
        git_out(&dir, &["status", "--porcelain"]),
        "",
        "the tree is clean: the build can be traced to the commit"
    );
    // The build saw the new version, not the old one.
    assert_eq!(
        std::fs::read_to_string(dir.join("dist/hoist-0.1.4-1004.apk")).unwrap(),
        "code=1004"
    );
}

#[quench_test::test]
fn a_dry_run_says_what_a_real_run_would_raise_and_changes_nothing() {
    let (dir, config) = repository("raise-dry", "0.1.3");
    let options = Options {
        dry_run: true,
        ..bump()
    };
    let (result, out, sent, _) = managed(&config, &dir, &options, (200, BOTH));
    result.unwrap();
    assert!(sent.is_empty());
    assert!(out.contains("would release 0.1.4"), "{out}");
    assert_eq!(read_version(&dir.join("Cargo.toml")).unwrap(), "0.1.3");
    assert!(!dir.join("dist").exists(), "nothing was built");
    assert_eq!(git_out(&dir, &["status", "--porcelain"]), "");
}

#[quench_test::test]
fn a_release_that_stopped_half_way_sends_only_what_is_missing() {
    let (dir, config) = repository("finish", "0.1.3");
    let (result, out, sent, _) = managed(&config, &dir, &bump(), (200, ANDROID_ONLY));
    result.unwrap();
    assert_eq!(sent.len(), 1, "{out}");
    assert!(sent[0].url.contains("/linux/1003?"), "{}", sent[0].url);
    assert!(
        out.contains("the others are sent now")
            && out.contains("[android] already in the warehouse, not built again"),
        "{out}"
    );
    assert!(
        !dir.join("dist/hoist-0.1.3-1003.apk").exists(),
        "the APK that is there was not built again"
    );
    assert_eq!(read_version(&dir.join("Cargo.toml")).unwrap(), "0.1.3");
}

#[quench_test::test]
fn a_dirty_tree_stops_before_the_version_is_touched() {
    let (dir, config) = repository("raise-dirty", "0.1.3");
    std::fs::write(dir.join("notes.txt"), "uncommitted").unwrap();
    let (result, _, sent, _) = managed(&config, &dir, &bump(), (200, BOTH));
    let why = result.unwrap_err();
    assert!(
        why.contains("uncommitted changes") && sent.is_empty(),
        "{why}"
    );
    assert_eq!(read_version(&dir.join("Cargo.toml")).unwrap(), "0.1.3");
}

#[quench_test::test]
fn a_warehouse_that_cannot_say_what_it_holds_stops_the_release() {
    let (dir, config) = repository("list-fails", "0.1.3");
    let (result, _, sent, _) = managed(&config, &dir, &bump(), (403, "no"));
    let why = result.unwrap_err();
    assert!(
        why.contains("could not say which versions") && why.contains("403") && sent.is_empty(),
        "{why}"
    );
}

#[quench_test::test]
fn a_version_is_raised_even_where_there_is_no_repository() {
    let (dir, config) = project("raise-no-git", "0.1.3");
    let (result, out, sent, _) = managed(&config, &dir, &bump(), (200, BOTH));
    result.unwrap();
    assert_eq!(sent.len(), 2, "{out}");
    assert!(
        out.contains("raised to 0.1.4") && !out.contains("committed"),
        "{out}"
    );
    assert_eq!(read_version(&dir.join("Cargo.toml")).unwrap(), "0.1.4");
}

fn two_programs(label: &str) -> (PathBuf, Config) {
    let (dir, mut config) = repository(label, "0.1.3");
    let second: Config = quench_serde::toml::from_str(
        r#"
[artifacts]
[artifacts.programs.daybook]
program = "dev.lorehaven.daybook"
version_file = "Cargo.toml"

[[artifacts.programs.daybook.targets]]
platform = "android"
format = "apk"
build = ["sh", "-c", "mkdir -p dist && printf d > dist/daybook-${version_name}.apk"]
file = "dist/daybook-${version_name}.apk"
"#,
    )
    .unwrap();
    config.artifacts.programs.extend(second.artifacts.programs);
    (dir, config)
}

fn all(
    config: &Config,
    root: &Path,
    options: &Options,
    yes: bool,
    confirm: bool,
    fail_on: Option<&str>,
) -> (Result<(), String>, String, Vec<Upload>, usize) {
    let sent: Sent = RefCell::default();
    let asked = RefCell::new(0_usize);
    let mut out = Vec::new();
    let result = release_all_with(
        config,
        root,
        options,
        yes,
        &mut out,
        &|| {
            *asked.borrow_mut() += 1;
            Ok(confirm)
        },
        &|| Ok("short-lived".to_string()),
        &|upload| {
            sent.borrow_mut().push(upload.clone());
            if fail_on.is_some_and(|f| upload.url.contains(f)) {
                return Ok((500, "boom".to_string()));
            }
            Ok((201, String::new()))
        },
        &|_, _| Ok((200, "[]".to_string())),
    )
    .map_err(|e| format!("{e:#}"));
    (
        result,
        String::from_utf8(out).unwrap(),
        sent.into_inner(),
        asked.into_inner(),
    )
}

#[quench_test::test]
fn every_program_is_released_in_name_order_and_asked_about_once() {
    let (dir, config) = two_programs("all");
    let (result, out, sent, asked) = all(&config, &dir, &bump(), false, true, None);
    result.unwrap();
    assert_eq!(asked, 1, "one question for the whole release");
    assert_eq!(sent.len(), 3, "{out}");
    let programs: Vec<&str> = sent
        .iter()
        .map(|u| {
            u.url
                .split("/artifacts/")
                .nth(1)
                .unwrap()
                .split('/')
                .next()
                .unwrap()
        })
        .collect();
    assert_eq!(
        programs,
        [
            "dev.lorehaven.daybook",
            "dev.lorehaven.hoist",
            "dev.lorehaven.hoist"
        ],
        "daybook before hoist"
    );
    assert!(out.contains("2 program(s): daybook, hoist"), "{out}");
}

#[quench_test::test]
fn without_yes_a_no_releases_nothing_and_with_yes_nobody_is_asked() {
    let (dir, config) = two_programs("all-confirm");
    let (result, out, sent, asked) = all(&config, &dir, &bump(), false, false, None);
    result.unwrap();
    assert!(
        sent.is_empty() && asked == 1 && out.contains("Release cancelled."),
        "{out}"
    );
    let (result, _, sent, asked) = all(&config, &dir, &bump(), true, false, None);
    result.unwrap();
    assert_eq!((sent.len(), asked), (3, 0));
}

#[quench_test::test]
fn the_first_program_that_fails_stops_the_rest_and_is_named() {
    let (dir, config) = two_programs("all-fails");
    let (result, _, sent, _) = all(&config, &dir, &bump(), true, true, Some("daybook"));
    let why = result.unwrap_err();
    assert!(
        why.contains("releasing daybook") && why.contains("HTTP 500"),
        "{why}"
    );
    assert_eq!(sent.len(), 1, "hoist was not started");
}

#[quench_test::test]
fn a_dry_run_of_all_asks_nothing_and_sends_nothing() {
    let (dir, config) = two_programs("all-dry");
    let options = Options {
        dry_run: true,
        ..bump()
    };
    let (result, out, sent, asked) = all(&config, &dir, &options, false, false, None);
    result.unwrap();
    assert!(
        sent.is_empty() && asked == 0 && out.contains("(dry run)"),
        "{out}"
    );
}

#[quench_test::test]
fn no_programs_is_an_error_not_a_quiet_success() {
    let (dir, mut config) = project("all-none", "0.1.3");
    config.artifacts.programs.clear();
    let (result, ..) = all(&config, &dir, &bump(), true, true, None);
    assert!(result.unwrap_err().contains("no artifacts"));
}
