use crate::env_support::cwd_lock;
use riveter::cli::SchemasCmd;
use riveter::env::{Workspace, with_workspace};
use riveter::render::{ResourceScope, Selector, render_to_string};
use riveter::schema::{self, Outcome, SchemaSet, Source, bundled};
use riveter::schema_cmd::{
    FetchRequest, check_documents, crds_from_kubectl, fetch, is_built_in, label, parse_documents,
    schemas_command, validate_env, validate_files,
};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

fn crd_json(group: &str, kind: &str, plural: &str) -> Value {
    json!({
        "apiVersion": "apiextensions.k8s.io/v1", "kind": "CustomResourceDefinition",
        "metadata": {"name": format!("{plural}.{group}")},
        "spec": {
            "group": group, "scope": "Namespaced",
            "names": {"kind": kind, "plural": plural},
            "versions": [{"name": "v1", "served": true, "storage": true, "schema": {"openAPIV3Schema": {
                "type": "object",
                "properties": {"apiVersion": {"type": "string"}, "kind": {"type": "string"}, "metadata": {"type": "object"},
                               "spec": {"type": "object", "properties": {"size": {"type": "integer"}}}}
            }}}],
        }
    })
}

// --- documents -----------------------------------------------------------

#[test]
fn every_document_of_a_multi_document_file_is_read() {
    let docs = parse_documents("kind: A\n---\nkind: B\n---\n---\nkind: C\n").unwrap();
    let kinds: Vec<&str> = docs.iter().map(|d| d["kind"].as_str().unwrap()).collect();
    assert_eq!(kinds, ["A", "B", "C"], "empty documents are skipped");
}

#[test]
fn a_list_is_flattened_into_its_items() {
    let text = "apiVersion: v1\nkind: List\nitems:\n  - {kind: A, metadata: {name: a}}\n  - {kind: B, metadata: {name: b}}\n";
    assert_eq!(parse_documents(text).unwrap().len(), 2);

    let typed = "kind: CertificateList\nitems:\n  - {kind: Certificate}\n";
    assert_eq!(parse_documents(typed).unwrap().len(), 1);

    // A resource that merely has a field called `items` is not a list.
    let not_list = "kind: Widget\nitems: [1, 2]\n";
    assert_eq!(parse_documents(not_list).unwrap().len(), 1);
}

#[test]
fn invalid_yaml_is_an_error_naming_the_document() {
    let err = parse_documents("kind: A\n---\nkind: [unclosed\n").unwrap_err();
    assert!(format!("{err:#}").contains("document 2"), "{err:#}");
}

#[test]
fn resources_are_named_kind_slash_name() {
    assert_eq!(
        label(&json!({"kind": "Certificate", "metadata": {"name": "api"}})),
        "certificate/api"
    );
    assert_eq!(label(&json!({})), "?/?");
}

#[test]
fn a_report_counts_valid_skipped_and_failed_separately() {
    let mut set = SchemaSet::empty();
    set.insert(
        schema::CrdSchema::from_crd(&crd_json("example.io", "Widget", "widgets")).unwrap(),
        Source::Cache,
    );
    let widget = |size: Value| json!({"apiVersion": "example.io/v1", "kind": "Widget", "metadata": {"name": "w"}, "spec": {"size": size}});
    let docs = vec![
        widget(json!(1)),
        widget(json!("x")),
        json!({"apiVersion": "apps/v1", "kind": "Deployment"}),
        json!({"apiVersion": "apps/v1", "kind": "Deployment"}),
        json!({"apiVersion": "v1", "kind": "Service"}),
        json!({"apiVersion": "networking.k8s.io/v1", "kind": "Ingress"}),
        json!({"apiVersion": "unknown.example/v1", "kind": "Gadget"}),
    ];

    let report = check_documents(&set, &docs);
    assert_eq!(report.valid, 1);
    assert_eq!(report.failures.len(), 1);
    assert_eq!(report.failures[0].0, "widget/w");
    // Built-in kinds are expected to have no CRD schema; only the unknown group is worth a warning.
    assert_eq!(report.built_in.get("deployment"), Some(&2));
    assert_eq!(report.built_in.get("service"), Some(&1));
    assert_eq!(report.built_in.get("ingress"), Some(&1));
    assert_eq!(report.skipped.len(), 1);
    assert_eq!(report.skipped.get("gadget"), Some(&1));
}

#[test]
fn built_in_groups_are_recognised_by_their_api_version() {
    for built_in in [
        "v1",
        "apps/v1",
        "batch/v1",
        "networking.k8s.io/v1",
        "rbac.authorization.k8s.io/v1",
        "apiextensions.k8s.io/v1",
    ] {
        assert!(is_built_in(built_in), "{built_in}");
    }
    // Custom resources - including ones under `k8s.io` - are not built in.
    for custom in [
        "cert-manager.io/v1",
        "traefik.io/v1alpha1",
        "gateway.networking.k8s.io/v1",
        "cilium.io/v2",
        "",
    ] {
        assert!(!is_built_in(custom) || custom.is_empty(), "{custom}");
    }
}

// --- what kubectl prints ---------------------------------------------------

#[test]
fn one_crd_is_an_object_and_several_are_a_list() {
    let one = crd_json("a.io", "A", "as").to_string();
    assert_eq!(crds_from_kubectl(&one).unwrap().len(), 1);

    let many = json!({"kind": "List", "items": [crd_json("a.io", "A", "as"), crd_json("b.io", "B", "bs")]}).to_string();
    assert_eq!(crds_from_kubectl(&many).unwrap().len(), 2);

    // `--ignore-not-found` with nothing found prints nothing, or an empty list.
    assert!(crds_from_kubectl("").unwrap().is_empty());
    assert!(crds_from_kubectl("  \n").unwrap().is_empty());
    assert!(
        crds_from_kubectl(r#"{"kind":"List","items":[]}"#)
            .unwrap()
            .is_empty()
    );
    assert!(crds_from_kubectl("not json").is_err());
}

// --- fetching, against a stand-in kubectl ------------------------------------

/// Puts a `kubectl` on `$PATH` that records its arguments and prints `output`, for the duration of `body`.
fn with_fake_kubectl<T>(
    output: &str,
    exit_code: i32,
    args_file: &Path,
    body: impl FnOnce() -> T,
) -> T {
    let _guard = cwd_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("kubectl");
    fs::write(
        &script,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\ncat <<'EOF_OUTPUT'{redirect}\n{output}\nEOF_OUTPUT\nexit {exit_code}\n",
            args_file.display(),
            // A failing kubectl says why on stderr.
            redirect = if exit_code == 0 { "" } else { " >&2" }
        ),
    )
    .unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();

    let original = std::env::var("PATH").unwrap_or_default();
    envmnt::set("PATH", format!("{}:{original}", dir.path().display()));
    let result = body();
    envmnt::set("PATH", &original);
    result
}

#[test]
fn fetch_asks_for_the_templated_crds_by_default_and_writes_one_file_each() {
    let out = tempfile::tempdir().unwrap();
    let args = out.path().join("args");
    let list = json!({"kind": "List", "items": [
        crd_json("cert-manager.io", "Certificate", "certificates"),
        crd_json("traefik.io", "Middleware", "middlewares"),
    ]});

    let fetched = with_fake_kubectl(&list.to_string(), 0, &args, || {
        fetch(&FetchRequest {
            output: Some(out.path().join("schemas")),
            ..FetchRequest::default()
        })
        .unwrap()
    });

    assert_eq!(fetched.len(), 2);
    assert!(
        out.path()
            .join("schemas/cert-manager.io_certificates.json")
            .is_file()
    );
    assert!(
        out.path()
            .join("schemas/traefik.io_middlewares.json")
            .is_file()
    );

    // The seven CRDs riveter templates, by name, tolerating ones the cluster lacks.
    let asked = fs::read_to_string(&args).unwrap();
    for name in [
        "certificates.cert-manager.io",
        "httproutes.gateway.networking.k8s.io",
        "middlewares.traefik.io",
    ] {
        assert!(
            asked.lines().any(|l| l == name),
            "{name} not requested: {asked}"
        );
    }
    assert!(asked.contains("--ignore-not-found"));

    // What was written round-trips through the loader.
    let mut set = SchemaSet::empty();
    set.load_dir(&out.path().join("schemas"), Source::Cache)
        .unwrap();
    assert_eq!(set.list().len(), 2);
}

#[test]
fn fetch_can_name_crds_take_everything_and_choose_a_context() {
    let out = tempfile::tempdir().unwrap();
    let args = out.path().join("args");
    let one = crd_json("a.io", "A", "as").to_string();

    with_fake_kubectl(&one, 0, &args, || {
        fetch(&FetchRequest {
            crds: vec!["as.a.io".to_string()],
            context: Some("staging".to_string()),
            output: Some(out.path().join("o")),
            ..FetchRequest::default()
        })
        .unwrap();
    });
    let asked: Vec<String> = fs::read_to_string(&args)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect();
    assert_eq!(asked[..2], ["--context", "staging"]);
    assert!(asked.contains(&"as.a.io".to_string()));
    assert!(
        !asked.contains(&"certificates.cert-manager.io".to_string()),
        "an explicit name replaces the default set"
    );

    with_fake_kubectl(&one, 0, &args, || {
        fetch(&FetchRequest {
            all: true,
            output: Some(out.path().join("all")),
            ..FetchRequest::default()
        })
        .unwrap();
    });
    let asked = fs::read_to_string(&args).unwrap();
    assert_eq!(
        asked.lines().collect::<Vec<_>>(),
        ["get", "crd", "-o", "json", "--ignore-not-found"]
    );
}

#[test]
fn fetch_fails_clearly_when_kubectl_does_or_nothing_comes_back() {
    let out = tempfile::tempdir().unwrap();
    let args = out.path().join("args");
    let request = || FetchRequest {
        output: Some(out.path().join("s")),
        ..FetchRequest::default()
    };

    let err = with_fake_kubectl("the connection to the server was refused", 1, &args, || {
        fetch(&request()).unwrap_err()
    });
    assert!(format!("{err:#}").contains("refused"), "{err:#}");

    let err = with_fake_kubectl("", 0, &args, || fetch(&request()).unwrap_err());
    assert!(
        format!("{err:#}").contains("no CRD with a schema"),
        "{err:#}"
    );
    assert!(
        !out.path().join("s").exists(),
        "nothing is written when nothing was fetched"
    );
}

#[test]
fn schemas_fetch_refuses_all_together_with_named_crds() {
    let err = schemas_command(&SchemasCmd::Fetch {
        crd: vec!["x.y".to_string()],
        all: true,
        context: None,
        output: None,
    })
    .unwrap_err();
    assert!(err.to_string().contains("use one"), "{err}");
}

// --- validating -------------------------------------------------------------

/// Validation reads the schema cache from the environment; point it somewhere empty so a developer's
/// real cache cannot change what a test sees.
fn with_empty_cache<T>(body: impl FnOnce() -> T) -> T {
    let _guard = cwd_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let dir = tempfile::tempdir().unwrap();
    let saved = std::env::var_os("RIVETER_SCHEMA_DIR");
    envmnt::set("RIVETER_SCHEMA_DIR", dir.path());
    let result = body();
    match saved {
        Some(v) => envmnt::set("RIVETER_SCHEMA_DIR", v),
        None => envmnt::remove("RIVETER_SCHEMA_DIR"),
    }
    result
}

fn workspace_with_overlay(overlay: &str) -> (tempfile::TempDir, Workspace) {
    let dir = tempfile::tempdir().unwrap();
    let overlays = dir.path().join("overlays");
    fs::create_dir_all(overlays.join("demo")).unwrap();
    fs::write(overlays.join("demo/overlay.yaml"), overlay).unwrap();
    let workspace = Workspace {
        overlays_dir: Some(overlays),
        output_dir: Some(dir.path().join("manifests")),
        env_vars: Some(HashMap::new()),
        ..Workspace::default()
    };
    (dir, workspace)
}

const GOOD_OVERLAY: &str = "\
env: demo
namespace_name: demo
resources:
  - kind: certificate
    name: api-tls
    secret_name: api-tls
    dns_names: [api.example.com]
    issuer:
      name: letsencrypt
      kind: ClusterIssuer
  - kind: deployment
    name: web
    image: nginx:1.27
";

#[test]
fn an_environment_whose_custom_resources_conform_validates() {
    let (dir, workspace) = workspace_with_overlay(GOOD_OVERLAY);
    with_empty_cache(|| {
        with_workspace(workspace, || {
            validate_env("demo", ResourceScope::All, &Selector::default())
        })
    })
    .unwrap();
    assert!(
        !dir.path().join("manifests").exists(),
        "validating writes nothing"
    );
}

#[test]
fn a_typo_in_a_raw_custom_resource_fails_the_environment() {
    let overlay = "\
env: demo
namespace_name: demo
resources:
  - kind: raw
    name: route
    manifest:
      apiVersion: traefik.io/v1alpha1
      kind: IngressRoute
      metadata: {name: route, namespace: demo}
      spec:
        routes:
          - mathc: Host(`a.example`)
            kind: Rule
            services: [{name: web, port: 80}]
";
    let (_dir, workspace) = workspace_with_overlay(overlay);
    let err = with_empty_cache(|| {
        with_workspace(workspace, || {
            validate_env("demo", ResourceScope::All, &Selector::default())
        })
    })
    .unwrap_err();
    assert!(
        err.to_string().contains("1 resource(s) failed validation"),
        "{err}"
    );
}

#[test]
fn targets_narrow_what_is_validated() {
    let overlay = format!(
        "{GOOD_OVERLAY}  - kind: raw\n    name: route\n    manifest:\n      apiVersion: traefik.io/v1alpha1\n      kind: IngressRoute\n      metadata: {{name: route}}\n      spec: {{typo: 1}}\n"
    );
    let (_dir, workspace) = workspace_with_overlay(&overlay);

    // The broken resource is outside what was asked for, so the run passes.
    with_empty_cache(|| {
        with_workspace(workspace.clone(), || {
            validate_env(
                "demo",
                ResourceScope::All,
                &Selector::parse(&["certificate"]).unwrap(),
            )
        })
    })
    .unwrap();

    // Named, it fails.
    assert!(
        with_empty_cache(|| {
            with_workspace(workspace, || {
                validate_env(
                    "demo",
                    ResourceScope::All,
                    &Selector::parse(&["raw"]).unwrap(),
                )
            })
        })
        .is_err()
    );
}

#[test]
fn files_are_validated_and_a_failure_in_any_fails_the_run() {
    let dir = tempfile::tempdir().unwrap();
    let good = dir.path().join("good.yaml");
    let bad = dir.path().join("bad.yaml");
    fs::write(&good, "apiVersion: cert-manager.io/v1\nkind: Certificate\nmetadata: {name: a}\nspec: {secretName: s, issuerRef: {name: i}}\n").unwrap();
    fs::write(&bad, "apiVersion: cert-manager.io/v1\nkind: Certificate\nmetadata: {name: b}\nspec: {secretName: s, issuerRef: {name: i}, dnsNmaes: [x]}\n").unwrap();

    with_empty_cache(|| validate_files(std::slice::from_ref(&good))).unwrap();
    assert!(with_empty_cache(|| validate_files(&[good.clone(), bad.clone()])).is_err());
    assert!(validate_files(&[dir.path().join("missing.yaml")]).is_err());
}

#[test]
fn a_fetched_schema_in_the_cache_is_what_validate_uses() {
    let cache = tempfile::tempdir().unwrap();
    let widget = schema::CrdSchema::from_crd(&crd_json("example.io", "Widget", "widgets")).unwrap();
    fs::write(
        cache.path().join(widget.file_name()),
        serde_json::to_string(&widget).unwrap(),
    )
    .unwrap();

    let doc = dir_file(
        "apiVersion: example.io/v1\nkind: Widget\nmetadata: {name: w}\nspec: {size: not-a-number}\n",
    );
    let _guard = cwd_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let saved = std::env::var_os("RIVETER_SCHEMA_DIR");
    envmnt::set("RIVETER_SCHEMA_DIR", cache.path());
    let with_cache = validate_files(std::slice::from_ref(&doc.1));
    envmnt::set("RIVETER_SCHEMA_DIR", doc.0.path());
    let without = validate_files(std::slice::from_ref(&doc.1));
    match saved {
        Some(v) => envmnt::set("RIVETER_SCHEMA_DIR", v),
        None => envmnt::remove("RIVETER_SCHEMA_DIR"),
    }

    assert!(with_cache.is_err(), "the cached schema rejects it");
    assert!(
        without.is_ok(),
        "with no schema held the kind is skipped, not failed"
    );
}

fn dir_file(text: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("doc.yaml");
    fs::write(&path, text).unwrap();
    (dir, path)
}

// --- riveter's own templates ---------------------------------------------------

/// The templates and the CRDs are maintained by different people at different times. Rendering every
/// golden fixture and checking what comes out against the real schemas is what notices a template
/// emitting something the cluster would reject - which `kubectl apply` would only say after the rest of
/// an overlay had gone out.
#[test]
fn what_riveters_own_templates_emit_satisfies_the_real_crd_schemas() {
    let set = bundled().unwrap();
    let vars: HashMap<String, String> = [
        ("IMAGE_TAG", "1.27"),
        ("DB_PASSWORD", "x"),
        ("NAMESPACE", "golden"),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect();

    let golden = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden");
    let mut checked: Vec<String> = Vec::new();
    let mut problems: Vec<String> = Vec::new();

    for entry in fs::read_dir(&golden).unwrap().flatten() {
        let path = entry.path();
        let Some(name) = path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_suffix(".overlay.yaml"))
        else {
            continue;
        };
        let rendered =
            render_to_string("golden", &fs::read_to_string(&path).unwrap(), &vars).unwrap();
        for doc in parse_documents(&rendered).unwrap() {
            match set.check(&doc) {
                Outcome::Valid => checked.push(label(&doc)),
                Outcome::Skipped => {}
                Outcome::Invalid(issues) => {
                    for issue in issues {
                        problems.push(format!("{name}: {}: {issue}", label(&doc)));
                    }
                }
            }
        }
    }

    assert!(
        problems.is_empty(),
        "templates emit what the CRDs reject:\n  {}",
        problems.join("\n  ")
    );
    // And the check really ran: every CRD-backed template the fixtures exercise was checked.
    for kind in [
        "certificate",
        "clusterissuer",
        "issuer",
        "gateway",
        "httproute",
        "ingressroute",
        "middleware",
    ] {
        assert!(
            checked.iter().any(|c| c.starts_with(&format!("{kind}/"))),
            "no {kind} was validated: {checked:?}"
        );
    }
}
