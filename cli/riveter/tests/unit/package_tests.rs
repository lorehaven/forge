use anyhow::{Result, anyhow};
use rivet_package::{Limits, Manifest, Package};
use riveter::env::{self, Workspace, overlay_dir, with_workspace};
use riveter::image_updates::ImageRef;
use riveter::package::{
    DigestResolver, InstallValues, PACKAGE_LABEL, PackOptions, VERSION_ANNOTATION,
    check_requirements, expand_suffix, literal_secret_values, merge_values, pack, parse_values,
    prepare_install, read_file, with_build_suffix,
};
use riveter::package_cmd::{PackageRef, parse_package_ref};
use riveter::render::{ResourceScope, Selector, generate_manifests_selected};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

const DIGEST_A: &str = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const DIGEST_B: &str = "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

fn write(root: &Path, path: &str, text: &str) {
    let target = root.join(path);
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(target, text).unwrap();
}

const OVERLAY: &str = r#"env: demo
namespace_name: demo-ns

resources:
{% include "demo/base.yaml.j2" %}
{% include "demo/app/deployment.yaml.j2" %}
"#;

const BASE: &str = "- kind: namespace\n  immutable: true\n";

const DEPLOYMENT: &str = "\
- kind: deployment
  name: api
  image: registry.example:5000/demo/api:1.2.3
  env_vars:
    TOKEN: ${API_TOKEN}
    REGION: ${REGION}
";

const MANIFEST: &str = "[package]\nname = \"demo\"\nversion = \"0.4.0\"\n";

/// An overlay directory under a fresh `overlays/`, returning the temp dir that holds it.
fn overlay() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("overlays");
    write(&root, "demo/overlay.yaml", OVERLAY);
    write(&root, "demo/base.yaml.j2", BASE);
    write(&root, "demo/app/deployment.yaml.j2", DEPLOYMENT);
    write(&root, "demo/rivet.toml", MANIFEST);
    write(&root, "demo/values.toml", "REGION = \"eu\"\n");
    write(&root, "demo/.env", "API_TOKEN=hunter2\n");
    write(&root, "demo/.env.example", "API_TOKEN=changeme\n");
    write(&root, "demo/.hidden/notes.txt", "private\n");
    dir
}

/// Answers every lookup with a fixed digest and remembers what it was asked.
struct Fixed {
    digest: &'static str,
    seen: RefCell<Vec<String>>,
}

impl Fixed {
    const fn new(digest: &'static str) -> Self {
        Self {
            digest,
            seen: RefCell::new(Vec::new()),
        }
    }
}

impl DigestResolver for Fixed {
    fn digest(&self, image: &ImageRef) -> Result<String> {
        self.seen.borrow_mut().push(image.original.clone());
        Ok(self.digest.to_string())
    }
}

struct Failing;

impl DigestResolver for Failing {
    fn digest(&self, _: &ImageRef) -> Result<String> {
        Err(anyhow!("registry unreachable"))
    }
}

const fn opts<'a>(root: &'a Path, out: &'a Path) -> PackOptions<'a> {
    PackOptions {
        env: "demo",
        overlays_dir: root,
        out_dir: out,
        version_suffix: None,
    }
}

fn read_back(path: &Path) -> Package {
    read_file(path).unwrap()
}

// --- pack -----------------------------------------------------------

#[test]
fn packs_the_overlay_directory_without_secrets() {
    let dir = overlay();
    let root = dir.path().join("overlays");
    let out = dir.path().join("out");

    let packed = pack(&opts(&root, &out), None).unwrap();

    assert_eq!(packed.path, out.join("demo-0.4.0.rivet"));
    assert_eq!(
        packed.manifest.package.namespace.as_deref(),
        Some("demo-ns")
    );

    let package = read_back(&packed.path);
    let files: Vec<_> = package.files.keys().map(String::as_str).collect();
    assert_eq!(
        files,
        [
            ".env.example",
            "SHA256SUMS",
            "app/deployment.yaml.j2",
            "base.yaml.j2",
            "overlay.yaml",
            "rivet.toml",
            "values.toml",
        ]
    );
    // The real `.env` and the hidden directory never travel.
    assert!(!package.files.contains_key(".env"));
    assert_eq!(packed.excluded, [".env"]);
    assert_eq!(
        packed.sha256,
        rivet_package::sha256_hex(&fs::read(&packed.path).unwrap())
    );
    assert_eq!(
        fs::read_to_string(out.join(".gitignore")).unwrap(),
        "# Written by riveter: packages are build output.\n*.rivet\n"
    );
}

#[test]
fn packing_is_reproducible() {
    let dir = overlay();
    let root = dir.path().join("overlays");

    let first = pack(&opts(&root, &dir.path().join("a")), None).unwrap();
    let second = pack(&opts(&root, &dir.path().join("b")), None).unwrap();

    assert_eq!(
        fs::read(first.path).unwrap(),
        fs::read(second.path).unwrap()
    );
}

#[test]
fn reports_variables_the_package_does_not_default() {
    let dir = overlay();
    let root = dir.path().join("overlays");

    let packed = pack(&opts(&root, &dir.path().join("out")), None).unwrap();

    // REGION has a default in values.toml; API_TOKEN must come from the installer.
    assert_eq!(packed.required_vars, ["API_TOKEN"]);
}

#[test]
fn appends_the_version_suffix_as_build_metadata() {
    let dir = overlay();
    let root = dir.path().join("overlays");
    let out = dir.path().join("out");

    let packed = pack(
        &PackOptions {
            version_suffix: Some("b123"),
            ..opts(&root, &out)
        },
        None,
    )
    .unwrap();

    assert_eq!(packed.manifest.package.version, "0.4.0+b123");
    assert_eq!(packed.path, out.join("demo-0.4.0+b123.rivet"));
    assert_eq!(
        read_back(&packed.path).manifest.package.version,
        "0.4.0+b123"
    );
}

#[test]
fn version_suffix_rules() {
    assert_eq!(with_build_suffix("1.2.3", "b1").unwrap(), "1.2.3+b1");
    assert_eq!(
        with_build_suffix("1.2.3-rc.1", "b.2").unwrap(),
        "1.2.3-rc.1+b.2"
    );
    assert!(with_build_suffix("1.2.3+old", "b1").is_err());
    assert!(with_build_suffix("1.2.3", "bad suffix").is_err());
    assert!(with_build_suffix("1.2.3", "").is_err());
    assert!(with_build_suffix("nope", "b1").is_err());
}

#[test]
fn an_explicit_namespace_in_the_manifest_is_kept() {
    let dir = overlay();
    let root = dir.path().join("overlays");
    write(
        &root,
        "demo/rivet.toml",
        "[package]\nname = \"demo\"\nversion = \"0.4.0\"\nnamespace = \"chosen\"\n",
    );

    let packed = pack(&opts(&root, &dir.path().join("out")), None).unwrap();
    assert_eq!(packed.manifest.package.namespace.as_deref(), Some("chosen"));
}

#[test]
fn refuses_a_missing_or_mismatched_manifest() {
    let dir = overlay();
    let root = dir.path().join("overlays");
    let out = dir.path().join("out");

    write(
        &root,
        "demo/rivet.toml",
        "[package]\nname = \"other\"\nversion = \"1.0.0\"\n",
    );
    let err = pack(&opts(&root, &out), None).unwrap_err();
    assert!(format!("{err:#}").contains("must match"), "{err:#}");

    fs::remove_file(root.join("demo/rivet.toml")).unwrap();
    let err = pack(&opts(&root, &out), None).unwrap_err();
    assert!(
        format!("{err:#}").contains("needs a `rivet.toml`"),
        "{err:#}"
    );
}

#[test]
fn refuses_an_overlay_directory_that_cannot_be_a_package_name() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("overlays");
    write(&root, "Bad_Name/overlay.yaml", "resources: []\n");

    let err = pack(
        &PackOptions {
            env: "Bad_Name",
            overlays_dir: &root,
            out_dir: &dir.path().join("out"),
            version_suffix: None,
        },
        None,
    )
    .unwrap_err();
    assert!(format!("{err:#}").contains("DNS-1123"), "{err:#}");
}

#[test]
fn refuses_an_overlay_without_overlay_yaml() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("overlays");
    fs::create_dir_all(root.join("demo")).unwrap();

    let err = pack(&opts(&root, &dir.path().join("out")), None).unwrap_err();
    assert!(format!("{err:#}").contains("overlay not found"), "{err:#}");
}

#[test]
fn refuses_an_include_from_outside_the_overlay_directory() {
    let dir = overlay();
    let root = dir.path().join("overlays");
    write(&root, "shared/common.yaml.j2", "- kind: namespace\n");
    write(
        &root,
        "demo/overlay.yaml",
        "env: demo\nresources:\n{% include \"shared/common.yaml.j2\" %}\n",
    );

    let err = pack(&opts(&root, &dir.path().join("out")), None).unwrap_err();
    let message = format!("{err:#}");
    assert!(message.contains("outside `demo/`"), "{message}");
    assert!(message.contains("shared/common.yaml.j2"), "{message}");
}

#[test]
fn refuses_an_include_of_a_file_that_would_not_be_packed() {
    let dir = overlay();
    let root = dir.path().join("overlays");
    write(&root, "demo/.private.yaml.j2", "- kind: namespace\n");
    write(
        &root,
        "demo/overlay.yaml",
        "env: demo\nresources:\n{% include \"demo/.private.yaml.j2\" %}\n",
    );

    let err = pack(&opts(&root, &dir.path().join("out")), None).unwrap_err();
    assert!(format!("{err:#}").contains("cannot be packed"), "{err:#}");
}

#[test]
fn includes_behind_a_condition_are_found_by_rendering() {
    let dir = overlay();
    let root = dir.path().join("overlays");
    write(&root, "other/extra.yaml.j2", "- kind: namespace\n");
    write(
        &root,
        "demo/overlay.yaml",
        "env: demo\nresources:\n{% if env == \"demo\" %}{% include \"other/extra.yaml.j2\" %}{% endif %}\n",
    );

    assert!(pack(&opts(&root, &dir.path().join("out")), None).is_err());
}

#[cfg(unix)]
#[test]
fn refuses_a_symlink() {
    let dir = overlay();
    let root = dir.path().join("overlays");
    std::os::unix::fs::symlink("/etc/passwd", root.join("demo/leak.txt")).unwrap();

    let err = pack(&opts(&root, &dir.path().join("out")), None).unwrap_err();
    assert!(format!("{err:#}").contains("symlink"), "{err:#}");
}

// --- pinning --------------------------------------------------------

#[test]
fn pins_each_image_to_its_digest_keeping_the_tag() {
    let dir = overlay();
    let root = dir.path().join("overlays");
    let resolver = Fixed::new(DIGEST_A);

    let packed = pack(&opts(&root, &dir.path().join("out")), Some(&resolver)).unwrap();

    assert_eq!(
        resolver.seen.borrow().as_slice(),
        ["registry.example:5000/demo/api:1.2.3"]
    );
    assert_eq!(packed.pinned.len(), 1);
    assert_eq!(packed.pinned[0].file, "app/deployment.yaml.j2");
    assert_eq!(packed.pinned[0].line, 3);
    assert_eq!(
        packed.pinned[0].to,
        format!("registry.example:5000/demo/api:1.2.3@{DIGEST_A}")
    );

    let package = read_back(&packed.path);
    let deployment = String::from_utf8(package.files["app/deployment.yaml.j2"].clone()).unwrap();
    assert!(
        deployment.contains(&format!(
            "  image: registry.example:5000/demo/api:1.2.3@{DIGEST_A}\n"
        )),
        "{deployment}"
    );
    // Only the image line changed.
    assert_eq!(deployment.replace(&format!("@{DIGEST_A}"), ""), DEPLOYMENT);
}

#[test]
fn looks_each_distinct_image_up_once() {
    let dir = overlay();
    let root = dir.path().join("overlays");
    write(
        &root,
        "demo/app/deployment.yaml.j2",
        "- kind: deployment\n  name: a\n  image: nginx:1.27\n- kind: deployment\n  name: b\n  image: nginx:1.27\n- kind: deployment\n  name: c\n  image: redis:7\n",
    );
    let resolver = Fixed::new(DIGEST_B);

    let packed = pack(&opts(&root, &dir.path().join("out")), Some(&resolver)).unwrap();

    assert_eq!(packed.pinned.len(), 3);
    assert_eq!(resolver.seen.borrow().as_slice(), ["nginx:1.27", "redis:7"]);
}

#[test]
fn leaves_alone_what_it_cannot_or_need_not_pin() {
    let dir = overlay();
    let root = dir.path().join("overlays");
    let already = format!("nginx:1.27@{DIGEST_A}");
    write(
        &root,
        "demo/app/deployment.yaml.j2",
        &format!(
            "- kind: deployment\n  name: a\n  image: {already}\n- kind: deployment\n  name: b\n  image: ${{IMAGE}}\n- kind: deployment\n  name: c\n  image: untagged\n"
        ),
    );
    let resolver = Fixed::new(DIGEST_B);

    let packed = pack(&opts(&root, &dir.path().join("out")), Some(&resolver)).unwrap();

    assert!(packed.pinned.is_empty());
    assert!(resolver.seen.borrow().is_empty());
    assert_eq!(packed.unpinned.len(), 2, "{:?}", packed.unpinned);
    assert!(packed.unpinned[0].contains("templated"));
    assert!(packed.unpinned[1].contains("no explicit tag"));

    let package = read_back(&packed.path);
    let deployment = String::from_utf8(package.files["app/deployment.yaml.j2"].clone()).unwrap();
    assert!(deployment.contains(&format!("image: {already}\n")));
}

#[test]
fn pinning_preserves_line_endings_and_a_missing_final_newline() {
    let dir = overlay();
    let root = dir.path().join("overlays");
    write(
        &root,
        "demo/app/deployment.yaml.j2",
        "- kind: deployment\r\n  name: a\r\n  image: nginx:1.27\r\n  port: 80",
    );

    let packed = pack(
        &opts(&root, &dir.path().join("out")),
        Some(&Fixed::new(DIGEST_A)),
    )
    .unwrap();

    let package = read_back(&packed.path);
    let text = String::from_utf8(package.files["app/deployment.yaml.j2"].clone()).unwrap();
    assert_eq!(
        text,
        format!("- kind: deployment\r\n  name: a\r\n  image: nginx:1.27@{DIGEST_A}\r\n  port: 80")
    );
}

#[test]
fn a_failed_lookup_stops_the_pack_and_names_the_way_out() {
    let dir = overlay();
    let root = dir.path().join("overlays");
    let out = dir.path().join("out");

    let err = pack(&opts(&root, &out), Some(&Failing)).unwrap_err();
    let message = format!("{err:#}");
    assert!(message.contains("--no-pin"), "{message}");
    assert!(message.contains("registry unreachable"), "{message}");
    assert!(!out.join("demo-0.4.0.rivet").exists());
}

#[test]
fn a_resolver_that_returns_something_else_is_rejected() {
    let dir = overlay();
    let root = dir.path().join("overlays");

    let err = pack(
        &opts(&root, &dir.path().join("out")),
        Some(&Fixed::new("not-a-digest")),
    )
    .unwrap_err();
    assert!(
        format!("{err:#}").contains("not a sha256 digest"),
        "{err:#}"
    );
}

// --- values ---------------------------------------------------------

#[test]
fn values_accept_scalars_and_reject_the_rest() {
    let values = parse_values("A = \"x\"\nB = 3\nC = true\nD = 1.5\n").unwrap();
    assert_eq!(
        values,
        BTreeMap::from([
            ("A".to_string(), "x".to_string()),
            ("B".to_string(), "3".to_string()),
            ("C".to_string(), "true".to_string()),
            ("D".to_string(), "1.5".to_string()),
        ])
    );

    assert!(parse_values("A = [1, 2]\n").is_err());
    assert!(parse_values("[A]\nb = 1\n").is_err());
    assert!(parse_values("\"bad key!\" = 1\n").is_err());
    assert!(parse_values("not toml").is_err());
}

fn package_with_values(values: &str) -> Package {
    let mut builder = rivet_package::PackageBuilder::new(Manifest::new("demo", "1.0.0"));
    builder
        .add_file("overlay.yaml", b"resources: []\n".to_vec())
        .unwrap();
    builder
        .add_file("values.toml", values.as_bytes().to_vec())
        .unwrap();
    builder.finish().unwrap()
}

#[test]
fn values_layer_defaults_then_env_file_then_set() {
    let package = package_with_values("A = \"default\"\nB = \"default\"\nC = \"default\"\n");
    let dir = tempfile::tempdir().unwrap();
    let env_file = dir.path().join("env");
    fs::write(
        &env_file,
        "# a comment\nB=\"from-file\"\nC=from-file\nD=only-file\n",
    )
    .unwrap();

    let merged = merge_values(
        &package,
        &InstallValues {
            env_file: Some(&env_file),
            sets: &["C=from-set".to_string(), "E=a=b".to_string()],
            ..Default::default()
        },
    )
    .unwrap();

    assert_eq!(merged["A"], "default");
    assert_eq!(merged["B"], "from-file");
    assert_eq!(merged["C"], "from-set");
    assert_eq!(merged["D"], "only-file");
    assert_eq!(merged["E"], "a=b");
}

#[test]
fn a_malformed_set_or_missing_env_file_is_an_error() {
    let package = package_with_values("");
    let sets = ["no-equals".to_string()];
    assert!(
        merge_values(
            &package,
            &InstallValues {
                env_file: None,
                sets: &sets,
                ..Default::default()
            }
        )
        .is_err()
    );
    let sets = ["=value".to_string()];
    assert!(
        merge_values(
            &package,
            &InstallValues {
                env_file: None,
                sets: &sets,
                ..Default::default()
            }
        )
        .is_err()
    );
    assert!(
        merge_values(
            &package,
            &InstallValues {
                env_file: Some(Path::new("/nonexistent/env")),
                sets: &[],
                ..Default::default()
            }
        )
        .is_err()
    );
}

// --- requirements ---------------------------------------------------

#[test]
fn requirements_gate_on_the_running_riveter() {
    let mut manifest = Manifest::new("demo", "1.0.0");
    manifest.requires.riveter = Some(">=0.3, <0.5".to_string());

    assert!(check_requirements(&manifest, "0.4.2").unwrap().is_empty());
    let err = check_requirements(&manifest, "0.2.11").unwrap_err();
    assert!(
        err.to_string().contains("requires riveter >=0.3, <0.5"),
        "{err}"
    );
}

#[test]
fn package_requirements_are_noted_not_enforced() {
    let mut manifest = Manifest::new("demo", "1.0.0");
    manifest.requires.packages = vec!["postgres >=1".to_string()];

    let notes = check_requirements(&manifest, "0.4.0").unwrap();
    assert_eq!(notes.len(), 1);
    assert!(notes[0].contains("not verified"));
}

// --- package references ---------------------------------------------

#[test]
fn parses_package_references() {
    assert_eq!(
        parse_package_ref("forge").unwrap(),
        PackageRef::Remote {
            name: "forge".into(),
            version: None
        }
    );
    assert_eq!(
        parse_package_ref("forge@0.4.0+b1").unwrap(),
        PackageRef::Remote {
            name: "forge".into(),
            version: Some("0.4.0+b1".into())
        }
    );
    assert_eq!(
        parse_package_ref("forge@latest").unwrap(),
        PackageRef::Remote {
            name: "forge".into(),
            version: None
        }
    );
    assert_eq!(
        parse_package_ref("./out/forge-0.4.0.rivet").unwrap(),
        PackageRef::File("./out/forge-0.4.0.rivet".into())
    );
    assert_eq!(
        parse_package_ref("FORGE.RIVET").unwrap(),
        PackageRef::File("FORGE.RIVET".into())
    );

    assert!(parse_package_ref("Forge").is_err());
    assert!(parse_package_ref("forge@1.0").is_err());
    assert!(parse_package_ref("").is_err());
    assert!(parse_package_ref("a b").is_err());
}

// --- install --------------------------------------------------------

fn installed(vars: &[(&str, &str)]) -> (riveter::package::Installation, tempfile::TempDir) {
    let dir = overlay();
    let root = dir.path().join("overlays");
    let packed = pack(&opts(&root, &dir.path().join("out")), None).unwrap();
    let package = read_back(&packed.path);
    let vars = vars
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect::<std::collections::HashMap<_, _>>();
    (
        prepare_install(&package, vars, std::collections::BTreeMap::new()).unwrap(),
        dir,
    )
}

#[test]
fn an_installed_package_renders_from_its_scratch_tree() {
    let (installation, _keep) = installed(&[("API_TOKEN", "s3cret"), ("REGION", "eu")]);

    let rendered = with_workspace(installation.workspace.clone(), || {
        generate_manifests_selected(&installation.env, ResourceScope::All, &Selector::default())
    })
    .unwrap();

    // Written under the scratch tree, not the working directory.
    let scratch_manifests = installation.workspace.output_dir.as_deref().unwrap();
    assert!(
        Path::new(&rendered.path).starts_with(scratch_manifests),
        "{} vs {}",
        rendered.path,
        scratch_manifests.display()
    );
    let manifest = fs::read_to_string(&rendered.path).unwrap();

    // Variables came from the install values, not from any `.env`.
    assert!(manifest.contains("s3cret"), "{manifest}");
    assert!(manifest.contains("eu"));
    // Provenance on every resource, whatever its template does with `labels`: the namespace
    // template, for one, writes its own metadata and ignores both.
    let docs: Vec<serde_yaml::Value> = serde_yaml::Deserializer::from_str(&manifest)
        .map(|doc| serde::Deserialize::deserialize(doc).unwrap())
        .collect();
    assert_eq!(docs.len(), 2, "{manifest}");
    for doc in &docs {
        let kind = doc["kind"].as_str().unwrap();
        assert_eq!(
            doc["metadata"]["labels"][PACKAGE_LABEL].as_str(),
            Some("demo"),
            "{kind} lacks the package label:\n{manifest}"
        );
        assert_eq!(
            doc["metadata"]["annotations"][VERSION_ANNOTATION].as_str(),
            Some("0.4.0"),
            "{kind} lacks the version annotation:\n{manifest}"
        );
    }
    assert_eq!(rendered.namespace.as_deref(), Some("demo-ns"));
}

#[test]
fn the_real_env_file_never_reaches_an_install() {
    let dir = overlay();
    let root = dir.path().join("overlays");
    let packed = pack(&opts(&root, &dir.path().join("out")), None).unwrap();
    let package = read_back(&packed.path);

    let merged = merge_values(&package, &InstallValues::default()).unwrap();

    // `.env` held API_TOKEN=hunter2; the package carries only REGION's default.
    assert!(!merged.contains_key("API_TOKEN"));
    assert_eq!(merged["REGION"], "eu");
}

#[test]
fn an_install_missing_a_variable_says_which() {
    let (installation, _keep) = installed(&[("REGION", "eu")]);

    let err = with_workspace(installation.workspace.clone(), || {
        generate_manifests_selected(&installation.env, ResourceScope::All, &Selector::default())
    })
    .unwrap_err();

    let message = format!("{err:#}");
    assert!(message.contains("API_TOKEN"), "{message}");
    assert!(message.contains("values given at install"), "{message}");
}

#[test]
fn a_workspace_is_scoped_to_the_call_and_restored_even_on_panic() {
    assert_eq!(overlay_dir(), Path::new("overlays"));

    let inside = with_workspace(
        Workspace {
            overlays_dir: Some("/somewhere".into()),
            ..Workspace::default()
        },
        overlay_dir,
    );
    assert_eq!(inside, Path::new("/somewhere"));
    assert_eq!(overlay_dir(), Path::new("overlays"));

    let result = std::panic::catch_unwind(|| {
        with_workspace(
            Workspace {
                overlays_dir: Some("/boom".into()),
                ..Workspace::default()
            },
            || panic!("inside"),
        )
    });
    assert!(result.is_err());
    assert_eq!(overlay_dir(), Path::new("overlays"));
    assert_eq!(env::output_dir(), Path::new("manifests"));
}

#[test]
fn workspaces_nest() {
    let outer = Workspace {
        overlays_dir: Some("/outer".into()),
        ..Workspace::default()
    };
    let inner = Workspace {
        overlays_dir: Some("/inner".into()),
        ..Workspace::default()
    };

    with_workspace(outer, || {
        assert_eq!(overlay_dir(), Path::new("/outer"));
        with_workspace(inner, || assert_eq!(overlay_dir(), Path::new("/inner")));
        assert_eq!(overlay_dir(), Path::new("/outer"));
    });
}

#[test]
fn a_package_file_that_is_not_valid_is_refused_with_its_path() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bad.rivet");
    fs::write(&path, b"not a package").unwrap();

    let err = read_file(&path).unwrap_err();
    assert!(format!("{err:#}").contains("bad.rivet"), "{err:#}");
    assert!(read_file(&dir.path().join("missing.rivet")).is_err());
}

#[test]
fn a_package_built_here_passes_the_registry_limits() {
    let dir = overlay();
    let root = dir.path().join("overlays");
    let packed = pack(&opts(&root, &dir.path().join("out")), None).unwrap();

    let bytes = fs::read(packed.path).unwrap();
    Package::read(std::io::Cursor::new(bytes), &Limits::default()).unwrap();
}

// --- version suffix tokens ------------------------------------------

fn at(timestamp: &str) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339(timestamp)
        .unwrap()
        .with_timezone(&chrono::Utc)
}

#[test]
fn suffix_tokens_expand_to_the_clock_and_the_commit() {
    let now = at("2026-10-02T13:55:07Z");

    assert_eq!(
        expand_suffix("{timestamp}", now, None).unwrap(),
        "20261002135507"
    );
    assert_eq!(
        expand_suffix("{timestamp}.{sha}", now, Some("1a2b3c4d5e6f")).unwrap(),
        "20261002135507.1a2b3c4"
    );
    assert_eq!(
        expand_suffix("ci-{sha}-x", now, Some("abc")).unwrap(),
        "ci-abc-x"
    );
}

#[test]
fn a_suffix_without_tokens_passes_through() {
    let now = at("2026-10-02T13:55:07Z");
    assert_eq!(expand_suffix("b123", now, None).unwrap(), "b123");
    assert_eq!(expand_suffix("", now, None).unwrap(), "");
}

#[test]
fn suffix_tokens_that_cannot_be_expanded_are_errors_not_literals() {
    let now = at("2026-10-02T13:55:07Z");

    let err = expand_suffix("{sha}", now, None).unwrap_err();
    assert!(err.to_string().contains("CONVEYOR_SHA"), "{err}");
    assert!(expand_suffix("{sha}", now, Some("")).is_err());

    let err = expand_suffix("{nope}", now, None).unwrap_err();
    assert!(err.to_string().contains("unknown token `{nope}`"), "{err}");

    assert!(expand_suffix("a{timestamp", now, None).is_err());
}

#[test]
fn an_expanded_suffix_is_valid_build_metadata_and_sorts_by_time() {
    let early = expand_suffix(
        "{timestamp}.{sha}",
        at("2026-10-02T09:00:00Z"),
        Some("ffffffff"),
    )
    .unwrap();
    let late = expand_suffix(
        "{timestamp}.{sha}",
        at("2026-10-02T10:00:00Z"),
        Some("00000000"),
    )
    .unwrap();

    let a = with_build_suffix("0.4.0", &early).unwrap();
    let b = with_build_suffix("0.4.0", &late).unwrap();

    // Whatever the commit hashes happen to be, the later build is the newer version.
    assert!(semver::Version::parse(&a).unwrap() < semver::Version::parse(&b).unwrap());
}

// --- secrets in the package -----------------------------------------

fn secret_overlay(body: &str) -> String {
    format!("env: demo\nresources:\n{body}")
}

#[test]
fn a_secret_made_of_placeholders_is_fine() {
    let overlay = secret_overlay(
        "  - kind: secret\n    name: db\n    string_data:\n      PASSWORD: ${DB_PASSWORD}\n      URL: postgres://u:${DB_PASSWORD}@db/app\n",
    );
    assert!(literal_secret_values(&overlay).is_empty());
}

#[test]
fn a_literal_secret_value_is_named_by_resource_and_key() {
    let overlay = secret_overlay(
        "  - kind: secret\n    name: db\n    string_data:\n      PASSWORD: hunter2\n      OK: ${FROM_ENV}\n  - kind: Secret\n    name: tls\n    data:\n      key: c2VjcmV0\n",
    );
    assert_eq!(
        literal_secret_values(&overlay),
        ["secret/db: PASSWORD", "secret/tls: key"]
    );
}

#[test]
fn an_escaped_placeholder_is_a_literal_and_so_is_a_non_string() {
    // `$${NAME}` stays literal text in the manifest, so it is not a placeholder.
    let overlay = secret_overlay(
        "  - kind: secret\n    name: s\n    string_data:\n      A: $${NOT_EXPANDED}\n      B: 1234\n      C: true\n      D: ~\n",
    );
    assert_eq!(
        literal_secret_values(&overlay),
        ["secret/s: A", "secret/s: B", "secret/s: C", "secret/s: D"]
    );
}

#[test]
fn both_spellings_of_string_data_are_checked() {
    let overlay = secret_overlay(
        "  - kind: secret\n    name: a\n    stringData:\n      K: literal\n  - kind: secret\n    name: b\n    string_data:\n      K: literal\n",
    );
    assert_eq!(
        literal_secret_values(&overlay),
        ["secret/a: K", "secret/b: K"]
    );
}

#[test]
fn other_kinds_and_a_secret_without_values_are_ignored() {
    let overlay = secret_overlay(
        "  - kind: configmap\n    name: c\n    data:\n      PASSWORD: not-a-secret-kind\n  - kind: secret\n    name: empty\n",
    );
    assert!(literal_secret_values(&overlay).is_empty());
}

#[test]
fn a_raw_secret_cannot_be_checked_so_it_is_refused() {
    let overlay = secret_overlay(
        "  - kind: raw\n    name: sneaky\n    manifest: |\n      apiVersion: v1\n      kind: Secret\n      stringData:\n        a: b\n",
    );
    let found = literal_secret_values(&overlay);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].starts_with("raw/sneaky"));

    // A raw resource that is not a Secret is left alone.
    let overlay =
        secret_overlay("  - kind: raw\n    name: fine\n    manifest: |\n      kind: ConfigMap\n");
    assert!(literal_secret_values(&overlay).is_empty());
}

#[test]
fn packing_refuses_an_overlay_that_writes_a_secret_into_the_package() {
    let dir = overlay();
    let root = dir.path().join("overlays");
    write(
        &root,
        "demo/overlay.yaml",
        "env: demo\nresources:\n  - kind: secret\n    name: db\n    string_data:\n      PASSWORD: hunter2\n",
    );

    let err = pack(&opts(&root, &dir.path().join("out")), None).unwrap_err();
    let message = format!("{err:#}");
    assert!(message.contains("secret/db: PASSWORD"), "{message}");
    assert!(message.contains("--env-file"), "{message}");
    assert!(!dir.path().join("out/demo-0.4.0.rivet").exists());
}

#[test]
fn packing_accepts_a_secret_that_is_a_placeholder() {
    let dir = overlay();
    let root = dir.path().join("overlays");
    write(
        &root,
        "demo/overlay.yaml",
        "env: demo\nresources:\n  - kind: secret\n    name: db\n    string_data:\n      PASSWORD: ${DB_PASSWORD}\n",
    );

    let packed = pack(&opts(&root, &dir.path().join("out")), None).unwrap();
    assert_eq!(packed.required_vars, ["DB_PASSWORD"]);
}

// --- variables an overlay needs -------------------------------------

#[test]
fn only_values_count_as_variables_not_comments_or_escapes() {
    let rendered = "\
# explains the escape: write `${...}` literally as `$${VAR}`
env: demo
resources:
  - kind: deployment
    name: web
    env_vars:
      A: ${ALPHA}
      B: prefix-${BETA}-suffix
      C: $${KEPT_LITERAL}
    args:
      - ${GAMMA}
";
    assert_eq!(
        riveter::render::overlay_vars(rendered),
        ["ALPHA", "BETA", "GAMMA"]
    );
}

#[test]
fn unparseable_overlays_fall_back_to_scanning_the_text() {
    assert_eq!(riveter::render::overlay_vars("a: [unclosed ${X}"), ["X"]);
}

fn render_with_replicas(replicas: &[(&str, u32)]) -> anyhow::Result<String> {
    let (mut installation, _keep) = installed(&[("API_TOKEN", "x"), ("REGION", "eu")]);
    installation.workspace.replicas = replicas
        .iter()
        .map(|(key, count)| ((*key).to_string(), *count))
        .collect();
    let rendered = with_workspace(installation.workspace.clone(), || {
        generate_manifests_selected(&installation.env, ResourceScope::All, &Selector::default())
    })?;
    Ok(fs::read_to_string(&rendered.path)?)
}

#[test]
fn replicas_given_at_install_win_over_the_package() {
    let stopped = render_with_replicas(&[("deployment/api", 0)]).unwrap();
    let docs: Vec<serde_yaml::Value> = serde_yaml::Deserializer::from_str(&stopped)
        .map(|doc| serde::Deserialize::deserialize(doc).unwrap())
        .collect();
    let deployment = docs
        .iter()
        .find(|d| d["kind"] == "Deployment")
        .expect("a deployment");
    assert_eq!(
        deployment["spec"]["replicas"].as_u64(),
        Some(0),
        "{stopped}"
    );

    let scaled = render_with_replicas(&[("Deployment/api", 4)]).unwrap();
    assert!(scaled.contains("replicas: 4"), "{scaled}");
}

#[test]
fn replicas_naming_something_the_package_does_not_declare_is_an_error() {
    let error = render_with_replicas(&[("deployment/nope", 0)])
        .unwrap_err()
        .to_string();
    assert!(error.contains("deployment/nope"), "{error}");
    assert!(error.contains("does not declare"), "{error}");
}

#[test]
fn replicas_only_apply_to_things_that_have_replicas() {
    let error = render_with_replicas(&[("namespace/demo-ns", 0)])
        .unwrap_err()
        .to_string();
    assert!(error.contains("deployment or statefulset"), "{error}");
}

#[test]
fn replicas_flags_are_parsed_strictly() {
    use riveter::package_cmd::parse_replicas;
    let parsed = parse_replicas(&[
        "Deployment/sage=0".to_string(),
        "statefulset/db=2".to_string(),
    ])
    .unwrap();
    assert_eq!(parsed["deployment/sage"], 0);
    assert_eq!(parsed["statefulset/db"], 2);
    for bad in [
        "sage=0",
        "deployment/sage",
        "deployment/sage=many",
        "/x=1",
        "deployment/=1",
    ] {
        assert!(parse_replicas(&[bad.to_string()]).is_err(), "{bad}");
    }
}

#[test]
fn a_deployment_naming_a_workload_the_overlay_lacks_is_refused_at_pack_time() {
    let dir = overlay();
    let root = dir.path().join("overlays");
    let manifest_path = root.join("demo/rivet.toml");
    let original = fs::read_to_string(&manifest_path).unwrap();

    fs::write(
        &manifest_path,
        format!("{original}\n[[deployment]]\nname = \"api\"\nresources = [\"deployment/api\"]\n"),
    )
    .unwrap();
    let packed = pack(&opts(&root, &dir.path().join("out")), None).unwrap();
    assert_eq!(read_back(&packed.path).manifest.deployments.len(), 1);

    fs::write(
        &manifest_path,
        format!("{original}\n[[deployment]]\nname = \"api\"\nresources = [\"deployment/nope\"]\n"),
    )
    .unwrap();
    let error = pack(&opts(&root, &dir.path().join("out2")), None)
        .unwrap_err()
        .to_string();
    assert!(error.contains("deployment/nope"), "{error}");
    assert!(error.contains("does not declare"), "{error}");
}

#[test]
fn except_leaves_a_resource_out_of_an_install_and_everything_else_in() {
    let (installation, _keep) = installed(&[("API_TOKEN", "x"), ("REGION", "eu")]);
    let selector = Selector::default().except(&["deployment/api"]).unwrap();

    let rendered = with_workspace(installation.workspace.clone(), || {
        generate_manifests_selected(&installation.env, ResourceScope::All, &selector)
    })
    .unwrap();
    let manifest = fs::read_to_string(&rendered.path).unwrap();
    assert!(!manifest.contains("kind: Deployment"), "{manifest}");
    assert!(manifest.contains("kind: Namespace"), "{manifest}");

    // Combined with targets, an exclusion still wins.
    let selector = Selector::parse(&["deployment"])
        .unwrap()
        .except(&["deployment/api"])
        .unwrap();
    assert!(!selector.matches("deployment", "api"));
    assert!(
        Selector::default()
            .except(&["secret/x"])
            .unwrap()
            .matches("deployment", "api")
    );
    assert!(Selector::default().except(&["a/b/c"]).is_err());
}

#[test]
fn the_inventory_lists_everything_the_package_declares_with_its_real_kinds_and_namespaces() {
    let (installation, _keep) = installed(&[("API_TOKEN", "x"), ("REGION", "eu")]);
    let line = with_workspace(installation.workspace.clone(), || {
        riveter::package_cmd::inventory_line(&installation.env)
    })
    .unwrap();

    let items: Vec<serde_json::Value> = serde_json::from_str(&line).unwrap();
    let find = |kind: &str| {
        items
            .iter()
            .find(|i| i["kind"] == kind)
            .unwrap_or_else(|| panic!("{kind} in {line}"))
    };
    assert_eq!(find("Deployment")["name"], "api");
    assert_eq!(find("Deployment")["apiVersion"], "apps/v1");
    assert_eq!(find("Deployment")["namespace"], "demo-ns");
    assert_eq!(find("Namespace")["name"], "demo-ns");
    assert!(
        find("Namespace")["namespace"].is_null(),
        "cluster-scoped kinds carry none"
    );
}
