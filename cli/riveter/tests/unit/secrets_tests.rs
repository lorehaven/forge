use crate::fake_cluster::{FakeCluster, with_cluster, with_clusters};
use quench_serde::Value;
use riveter::secrets::{
    HASH_ANNOTATION, KEY, LABEL, SyncRequest, manifest, overlays_with_values, secret_name, sync,
};
use std::fs;
use std::path::{Path, PathBuf};

const SECRET_VALUE: &str = "hunter2-do-not-leak";

/// An `overlays/` directory with the named overlays; `Some(text)` gives one a `.env`.
fn overlays(layout: &[(&str, Option<&str>)]) -> (quench_core::tmp::TempDir, PathBuf) {
    let dir = quench_core::tmp::TempDir::new().unwrap();
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

/// A cluster to sync to, and what was applied to it.
struct Recorder {
    cluster: FakeCluster,
}

fn recorder() -> Recorder {
    Recorder {
        cluster: FakeCluster::start(),
    }
}

impl Recorder {
    /// What was applied, as `apply secrets/<name>`; the reads that come before an apply are left out.
    fn calls(&self) -> Vec<String> {
        self.cluster
            .calls()
            .into_iter()
            .filter(|c| c.starts_with("apply "))
            .collect()
    }

    fn secret(&self, namespace: &str, package: &str) -> Value {
        self.cluster
            .object("secrets", namespace, &secret_name(package))
            .unwrap_or_else(|| panic!("no secret for {package} in {namespace}"))
    }
}

fn stored(manifest: &Value) -> String {
    let encoded = manifest["data"][KEY].as_str().unwrap();
    String::from_utf8(quench_core::base64::STANDARD.decode(encoded).unwrap()).unwrap()
}

// --- the Secret ---------------------------------------------------------------

#[quench_test::test]
fn the_secret_is_named_for_its_package() {
    assert_eq!(secret_name("media"), "gantry-values-media");
}

#[quench_test::test]
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
        quench_core::hex::encode(quench_crypto::hash::sha256(contents))
    );
    assert_eq!(m["metadata"]["labels"][LABEL], "media");
}

#[quench_test::test]
fn the_secret_carries_no_label_prune_could_select_it_by() {
    // `prune` selects on `app.kubernetes.io/managed-by=riveter` and `env=<env>`. A values Secret lives in
    // whatever namespace Gantry runs in - an overlay's own, perhaps - and must never look like its resource.
    let m = manifest("forge", "forge", b"A=1\n");
    let labels = m["metadata"]["labels"].as_object().unwrap();
    assert_eq!(labels.len(), 1, "{labels:?}");
    assert!(!labels.contains_key("app.kubernetes.io/managed-by"));
    assert!(!labels.contains_key("env"));
}

#[quench_test::test]
fn the_hash_changes_when_a_value_does_and_not_otherwise() {
    let hash =
        |text: &[u8]| manifest("a", "n", text)["metadata"]["annotations"][HASH_ANNOTATION].clone();
    assert_eq!(hash(b"A=1\n"), hash(b"A=1\n"));
    assert_ne!(hash(b"A=1\n"), hash(b"A=2\n"));
}

#[quench_test::test]
fn awkward_bytes_are_stored_exactly() {
    let contents = "TOKEN='it'\"s\"=a=b\nUNICODE=zażółć\nNO_NEWLINE=at-the-end".as_bytes();
    assert_eq!(stored(&manifest("a", "n", contents)).as_bytes(), contents);
}

// --- finding overlays -----------------------------------------------------------

#[quench_test::test]
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

#[quench_test::test]
fn syncing_one_overlay_applies_its_secret() {
    let (_dir, root) = overlays(&[("media", Some(&format!("PORT=80\nTOKEN={SECRET_VALUE}\n")))]);

    let kubectl = recorder();
    let synced = with_cluster(&kubectl.cluster, || {
        sync(&request(&["media"]), &root).unwrap()
    });

    assert_eq!(
        kubectl.calls(),
        ["apply secrets/gantry-values-media"],
        "exactly one apply"
    );
    let sent = kubectl.secret("forge", "media");
    assert_eq!(sent["metadata"]["name"], "gantry-values-media");
    assert_eq!(stored(&sent), format!("PORT=80\nTOKEN={SECRET_VALUE}\n"));

    assert_eq!(synced.len(), 1);
    assert_eq!(synced[0].package, "media");
    assert_eq!(synced[0].secret, "gantry-values-media");
    assert_eq!(synced[0].namespace, "forge");
    assert_eq!(synced[0].variables, ["PORT", "TOKEN"], "names, sorted");
}

#[quench_test::test]
fn a_value_never_appears_in_a_url_or_in_what_is_reported() {
    let (_dir, root) = overlays(&[("media", Some(&format!("TOKEN={SECRET_VALUE}\n")))]);

    let kubectl = recorder();
    let synced = with_cluster(&kubectl.cluster, || {
        sync(&request(&["media"]), &root).unwrap()
    });
    assert!(
        !kubectl.cluster.calls().join("\n").contains(SECRET_VALUE),
        "a value reached a request line"
    );
    // The report type has no field for a value, so printing all of it cannot leak one.
    assert!(!format!("{synced:?}").contains(SECRET_VALUE));
}

#[quench_test::test]
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

    let (ambient, staging) = (recorder(), recorder());
    let synced = with_clusters(
        &[("dev", &ambient.cluster), ("staging", &staging.cluster)],
        || sync(&all, &root).unwrap(),
    );

    assert_eq!(
        synced
            .iter()
            .map(|s| s.package.as_str())
            .collect::<Vec<_>>(),
        ["forge", "media"]
    );
    assert_eq!(
        staging.calls(),
        [
            "apply secrets/gantry-values-forge",
            "apply secrets/gantry-values-media"
        ]
    );
    assert_eq!(ambient.cluster.first_asked(), None);
    assert_eq!(
        staging.secret("gantry", "forge")["metadata"]["namespace"],
        "gantry"
    );
}

#[quench_test::test]
fn a_dry_run_reports_without_reaching_the_cluster() {
    let (_dir, root) = overlays(&[("media", Some("A=1\nB=2\n"))]);
    let mut dry = request(&["media"]);
    dry.dry_run = true;

    let kubectl = recorder();
    with_cluster(&kubectl.cluster, || {
        let synced = sync(&dry, &root).unwrap();
        assert_eq!(synced[0].variables, ["A", "B"]);
    });
    assert!(
        kubectl.cluster.calls().is_empty(),
        "{:?}",
        kubectl.cluster.calls()
    );
}

#[quench_test::test]
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

#[quench_test::test]
fn an_overlay_that_cannot_be_synced_is_refused_before_anything_is_written() {
    let (_dir, root) = overlays(&[
        ("media", Some("A=1\n")),
        ("vpn", None),
        ("empty", Some("# only a comment\n\n")),
    ]);

    let kubectl = recorder();
    with_cluster(&kubectl.cluster, || {
        let err = sync(&request(&["nope"]), &root).unwrap_err().to_string();
        assert!(err.contains("overlay not found"), "{err}");

        let err = sync(&request(&["vpn"]), &root).unwrap_err().to_string();
        assert!(err.contains("has no .env"), "{err}");

        let err = sync(&request(&["empty"]), &root).unwrap_err().to_string();
        assert!(err.contains("defines no variables"), "{err}");

        // The first overlay is fine; the second is not. The run stops, but the first has been synced.
        assert!(sync(&request(&["media", "vpn"]), &root).is_err());
    });
    assert_eq!(kubectl.calls().len(), 1);
}

#[quench_test::test]
fn an_overlay_that_cannot_be_a_package_name_has_no_secret() {
    let (_dir, root) = overlays(&[("Bad_Name", Some("A=1\n"))]);
    let err = sync(&request(&["Bad_Name"]), &root)
        .unwrap_err()
        .to_string();
    assert!(err.contains("DNS-1123"), "{err}");
}

#[quench_test::test]
fn a_namespace_is_required() {
    let (_dir, root) = overlays(&[("media", Some("A=1\n"))]);
    let mut blank = request(&["media"]);
    blank.namespace = "  ".to_string();
    assert!(sync(&blank, &root).is_err());
}

#[quench_test::test]
fn a_refusal_by_the_cluster_names_the_overlay_and_says_why() {
    let (_dir, root) = overlays(&[("media", Some("A=1\n"))]);
    let kubectl = recorder();
    kubectl.cluster.refuses("secret/gantry-values-media");
    with_cluster(&kubectl.cluster, || {
        let err = format!("{:#}", sync(&request(&["media"]), &root).unwrap_err());
        assert!(err.contains("syncing media"), "{err}");
        assert!(err.contains("refused"), "{err}");
    });
}

#[quench_test::test]
fn syncing_twice_is_the_same_apply_twice() {
    // `apply` is idempotent, so a second sync is not an error and leaves the same Secret.
    let (_dir, root) = overlays(&[("media", Some("A=1\n"))]);
    let kubectl = recorder();
    with_cluster(&kubectl.cluster, || {
        sync(&request(&["media"]), &root).unwrap();
        let first = kubectl.secret("forge", "media");
        sync(&request(&["media"]), &root).unwrap();
        assert_eq!(kubectl.secret("forge", "media"), first);
    });
    assert_eq!(kubectl.calls().len(), 2);
}
