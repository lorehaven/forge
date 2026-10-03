use crate::env_support::cwd_lock;
use base64::Engine as _;
use riveter::secrets::{
    HASH_ANNOTATION, KEY, LABEL, SyncRequest, manifest, overlays_with_values, secret_name, sync,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

const SECRET_VALUE: &str = "hunter2-do-not-leak";

/// An `overlays/` directory with the named overlays; `Some(text)` gives one a `.env`.
fn overlays(layout: &[(&str, Option<&str>)]) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("overlays");
    for (name, env) in layout {
        fs::create_dir_all(root.join(name)).unwrap();
        fs::write(root.join(name).join("overlay.yaml"), "resources: []\n").unwrap();
        if let Some(text) = env {
            fs::write(root.join(name).join(".env"), text).unwrap();
        }
    }
    (dir, root)
}

fn request(overlays: &[&str]) -> SyncRequest {
    SyncRequest {
        overlays: overlays.iter().map(ToString::to_string).collect(),
        namespace: "forge".to_string(),
        ..SyncRequest::default()
    }
}

/// A `kubectl` that records its arguments (one call per line) and what it was sent on stdin, and may fail.
struct Recorder {
    args: PathBuf,
    stdin: PathBuf,
    _dir: tempfile::TempDir,
}

fn with_kubectl<T>(exit_code: i32, body: impl FnOnce(&Recorder) -> T) -> T {
    let _guard = cwd_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let dir = tempfile::tempdir().unwrap();
    let recorder = Recorder {
        args: dir.path().join("args.log"),
        stdin: dir.path().join("stdin.log"),
        _dir: tempfile::tempdir().unwrap(),
    };
    let script = format!(
        "#!/bin/sh\necho \"$@\" >> '{}'\ncat >> '{}'\nprintf '\\n---\\n' >> '{}'\n[ {exit_code} -ne 0 ] && echo 'the connection to the server was refused' >&2\nexit {exit_code}\n",
        recorder.args.display(),
        recorder.stdin.display(),
        recorder.stdin.display(),
    );
    let path = dir.path().join("kubectl");
    fs::write(&path, script).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();

    let original = std::env::var("PATH").unwrap_or_default();
    envmnt::set("PATH", format!("{}:{original}", dir.path().display()));
    let result = body(&recorder);
    envmnt::set("PATH", &original);
    drop(dir);
    result
}

impl Recorder {
    fn calls(&self) -> Vec<String> {
        fs::read_to_string(&self.args)
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }
    fn manifests(&self) -> Vec<Value> {
        fs::read_to_string(&self.stdin)
            .unwrap_or_default()
            .split("\n---\n")
            .filter(|chunk| !chunk.trim().is_empty())
            .map(|chunk| serde_json::from_str(chunk.trim()).unwrap())
            .collect()
    }
}

fn stored(manifest: &Value) -> String {
    let encoded = manifest["data"][KEY].as_str().unwrap();
    String::from_utf8(
        base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .unwrap(),
    )
    .unwrap()
}

// --- the Secret ---------------------------------------------------------------

#[test]
fn the_secret_is_named_for_its_package() {
    assert_eq!(secret_name("media"), "gantry-values-media");
}

#[test]
fn the_manifest_holds_the_file_verbatim_and_a_hash_of_it() {
    let contents = b"A=1\nB=\"two words\"\n";
    let m = manifest("media", "forge", contents);

    assert_eq!(m["kind"], "Secret");
    assert_eq!(m["type"], "Opaque");
    assert_eq!(m["metadata"]["name"], "gantry-values-media");
    assert_eq!(m["metadata"]["namespace"], "forge");
    assert_eq!(stored(&m), "A=1\nB=\"two words\"\n");
    assert_eq!(
        m["metadata"]["annotations"][HASH_ANNOTATION],
        hex::encode(Sha256::digest(contents))
    );
    assert_eq!(m["metadata"]["labels"][LABEL], "media");
}

#[test]
fn the_secret_carries_no_label_prune_could_select_it_by() {
    // `prune` selects on `app.kubernetes.io/managed-by=riveter` and `env=<env>`. A values Secret lives in
    // whatever namespace Gantry runs in - an overlay's own, perhaps - and must never look like its resource.
    let m = manifest("forge", "forge", b"A=1\n");
    let labels = m["metadata"]["labels"].as_object().unwrap();
    assert_eq!(labels.len(), 1, "{labels:?}");
    assert!(!labels.contains_key("app.kubernetes.io/managed-by"));
    assert!(!labels.contains_key("env"));
}

#[test]
fn the_hash_changes_when_a_value_does_and_not_otherwise() {
    let hash =
        |text: &[u8]| manifest("a", "n", text)["metadata"]["annotations"][HASH_ANNOTATION].clone();
    assert_eq!(hash(b"A=1\n"), hash(b"A=1\n"));
    assert_ne!(hash(b"A=1\n"), hash(b"A=2\n"));
}

#[test]
fn awkward_bytes_are_stored_exactly() {
    let contents = "TOKEN='it'\"s\"=a=b\nUNICODE=zażółć\nNO_NEWLINE=at-the-end".as_bytes();
    assert_eq!(stored(&manifest("a", "n", contents)).as_bytes(), contents);
}

// --- finding overlays -----------------------------------------------------------

#[test]
fn only_overlays_with_both_a_manifest_and_a_dotenv_are_listed() {
    let (dir, root) = overlays(&[
        ("media", Some("A=1\n")),
        ("vpn", None),
        ("forge", Some("B=2\n")),
    ]);
    // A `.env` with no overlay.yaml beside it is not an overlay.
    fs::create_dir_all(root.join("stray")).unwrap();
    fs::write(root.join("stray/.env"), "C=3\n").unwrap();

    assert_eq!(overlays_with_values(&root).unwrap(), ["forge", "media"]);
    drop(dir);
    assert!(overlays_with_values(Path::new("/nonexistent/overlays")).is_err());
}

// --- syncing ---------------------------------------------------------------------

#[test]
fn syncing_one_overlay_sends_its_secret_to_kubectl_over_stdin() {
    let (_dir, root) = overlays(&[("media", Some(&format!("PORT=80\nTOKEN={SECRET_VALUE}\n")))]);

    let synced = with_kubectl(0, |kubectl| {
        let synced = sync(&request(&["media"]), &root).unwrap();

        assert_eq!(
            kubectl.calls(),
            ["apply -f -"],
            "exactly one call, reading the manifest from stdin"
        );
        let sent = kubectl.manifests();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0]["metadata"]["name"], "gantry-values-media");
        assert_eq!(stored(&sent[0]), format!("PORT=80\nTOKEN={SECRET_VALUE}\n"));
        synced
    });

    assert_eq!(synced.len(), 1);
    assert_eq!(synced[0].package, "media");
    assert_eq!(synced[0].secret, "gantry-values-media");
    assert_eq!(synced[0].namespace, "forge");
    assert_eq!(synced[0].variables, ["PORT", "TOKEN"], "names, sorted");
}

#[test]
fn a_value_never_appears_in_an_argument_or_in_what_is_reported() {
    let (_dir, root) = overlays(&[("media", Some(&format!("TOKEN={SECRET_VALUE}\n")))]);

    with_kubectl(0, |kubectl| {
        let synced = sync(&request(&["media"]), &root).unwrap();

        assert!(
            !kubectl.calls().join("\n").contains(SECRET_VALUE),
            "a value reached kubectl's command line"
        );
        // The report type has no field for a value, so printing all of it cannot leak one.
        assert!(!format!("{synced:?}").contains(SECRET_VALUE));
    });
}

#[test]
fn all_syncs_every_overlay_that_has_values_and_a_context_comes_first() {
    let (_dir, root) = overlays(&[
        ("media", Some("A=1\n")),
        ("vpn", None),
        ("forge", Some("B=2\n")),
    ]);
    let mut all = request(&[]);
    all.all = true;
    all.context = Some("staging".to_string());
    all.namespace = "gantry".to_string();

    with_kubectl(0, |kubectl| {
        let synced = sync(&all, &root).unwrap();

        assert_eq!(
            synced
                .iter()
                .map(|s| s.package.as_str())
                .collect::<Vec<_>>(),
            ["forge", "media"]
        );
        assert_eq!(
            kubectl.calls(),
            [
                "--context staging apply -f -",
                "--context staging apply -f -"
            ]
        );
        let sent = kubectl.manifests();
        assert_eq!(sent[0]["metadata"]["namespace"], "gantry");
        assert_eq!(sent[1]["metadata"]["name"], "gantry-values-media");
    });
}

#[test]
fn a_dry_run_reports_without_calling_kubectl() {
    let (_dir, root) = overlays(&[("media", Some("A=1\nB=2\n"))]);
    let mut dry = request(&["media"]);
    dry.dry_run = true;

    with_kubectl(0, |kubectl| {
        let synced = sync(&dry, &root).unwrap();
        assert_eq!(synced[0].variables, ["A", "B"]);
        assert!(kubectl.calls().is_empty(), "{:?}", kubectl.calls());
    });
}

#[test]
fn naming_overlays_and_all_together_or_neither_is_refused() {
    let (_dir, root) = overlays(&[("media", Some("A=1\n"))]);

    let mut both = request(&["media"]);
    both.all = true;
    assert!(
        sync(&both, &root)
            .unwrap_err()
            .to_string()
            .contains("not both, not neither")
    );
    assert!(
        sync(&request(&[]), &root)
            .unwrap_err()
            .to_string()
            .contains("not both, not neither")
    );
}

#[test]
fn an_overlay_that_cannot_be_synced_is_refused_before_anything_is_written() {
    let (_dir, root) = overlays(&[
        ("media", Some("A=1\n")),
        ("vpn", None),
        ("empty", Some("# only a comment\n\n")),
    ]);

    with_kubectl(0, |kubectl| {
        let err = sync(&request(&["nope"]), &root).unwrap_err().to_string();
        assert!(err.contains("overlay not found"), "{err}");

        let err = sync(&request(&["vpn"]), &root).unwrap_err().to_string();
        assert!(err.contains("has no .env"), "{err}");

        let err = sync(&request(&["empty"]), &root).unwrap_err().to_string();
        assert!(err.contains("defines no variables"), "{err}");

        // The first overlay is fine; the second is not. The run stops, but the first has been synced.
        assert!(sync(&request(&["media", "vpn"]), &root).is_err());
        assert_eq!(kubectl.calls().len(), 1);
    });
}

#[test]
fn an_overlay_that_cannot_be_a_package_name_has_no_secret() {
    let (_dir, root) = overlays(&[("Bad_Name", Some("A=1\n"))]);
    let err = sync(&request(&["Bad_Name"]), &root)
        .unwrap_err()
        .to_string();
    assert!(err.contains("DNS-1123"), "{err}");
}

#[test]
fn a_namespace_is_required() {
    let (_dir, root) = overlays(&[("media", Some("A=1\n"))]);
    let mut blank = request(&["media"]);
    blank.namespace = "  ".to_string();
    assert!(sync(&blank, &root).is_err());
}

#[test]
fn a_kubectl_failure_names_the_overlay_and_says_why() {
    let (_dir, root) = overlays(&[("media", Some("A=1\n"))]);
    with_kubectl(1, |_| {
        let err = format!("{:#}", sync(&request(&["media"]), &root).unwrap_err());
        assert!(err.contains("syncing media"), "{err}");
        assert!(
            err.contains("connection to the server was refused"),
            "{err}"
        );
    });
}

#[test]
fn syncing_twice_is_the_same_apply_twice() {
    // `apply` is idempotent, so a second sync is not an error and sends the same Secret.
    let (_dir, root) = overlays(&[("media", Some("A=1\n"))]);
    with_kubectl(0, |kubectl| {
        sync(&request(&["media"]), &root).unwrap();
        sync(&request(&["media"]), &root).unwrap();
        let sent = kubectl.manifests();
        assert_eq!(sent.len(), 2);
        assert_eq!(sent[0], sent[1]);
    });
}
