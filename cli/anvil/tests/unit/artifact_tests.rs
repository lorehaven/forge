use std::cell::RefCell;
use std::path::{Path, PathBuf};

use anvil::commands::artifact::{
    Options, Upload, expand, read_version, release_with, upload_url, version_code,
};
use anvil::config::Config;

fn temp(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("anvil-artifact-{}-{label}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A project with a version and the two builds pedlar has: an APK and a `.deb`, each made by a tiny shell command.
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

[artifacts.programs.pedlar]
program = "dev.lorehaven.pedlar"
version_file = "Cargo.toml"

[[artifacts.programs.pedlar.targets]]
platform = "android"
format = "apk"
build = ["sh", "-c", "mkdir -p dist && printf 'code=%s' $ANVIL_VERSION_CODE > dist/pedlar-${version_name}-${version_code}.apk"]
file = "dist/pedlar-${version_name}-${version_code}.apk"

[[artifacts.programs.pedlar.targets]]
name = "linux-deb"
platform = "linux"
format = "deb"
arch = "x86_64"
label = "Pedlar App"
build = ["sh", "-c", "mkdir -p dist && printf deb > dist/pedlar_${version_name}_amd64.deb"]
file = "dist/pedlar_${version_name}_amd64.deb"
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
        "pedlar",
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
    let program = &config.artifacts.programs["pedlar"];
    // Android: the APK carries its own identity, so there is no format to say.
    let apk = upload_url(
        "https://w.example/warehouse/",
        &program.program,
        &program.targets[0],
        1_003,
        "0.1.3",
        "pedlar.apk",
    );
    assert_eq!(
        apk,
        "https://w.example/warehouse/api/v1/artifacts/dev.lorehaven.pedlar/android/1003?format=apk&version_name=0.1.3&filename=pedlar.apk"
    );
    let deb = upload_url(
        "https://w.example/warehouse",
        &program.program,
        &program.targets[1],
        1_003,
        "0.1.3",
        "pedlar_0.1.3_amd64.deb",
    );
    assert!(
        deb.contains("/linux/1003?format=deb&arch=x86_64&version_name=0.1.3&label=Pedlar%20App&filename=pedlar_0.1.3_amd64.deb"),
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
        "https://warehouse.example/warehouse/api/v1/artifacts/dev.lorehaven.pedlar/android/2005?"
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
        dir.join("dist/pedlar-0.1.0-1000.apk").is_file(),
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
        why.contains("no artifact `nope`") && why.contains("known: pedlar"),
        "{why}"
    );
    // A build that fails, and one that leaves no file.
    config.artifacts.programs.get_mut("pedlar").unwrap().targets[0].build = vec!["false".into()];
    let (result, _, sent, _) = run(&config, &dir, &Options::default(), 201);
    assert!(result.unwrap_err().contains("failed"));
    assert!(sent.is_empty());
    config.artifacts.programs.get_mut("pedlar").unwrap().targets[0].build = vec!["true".into()];
    config.artifacts.programs.get_mut("pedlar").unwrap().targets[0].file =
        "dist/missing.apk".into();
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
