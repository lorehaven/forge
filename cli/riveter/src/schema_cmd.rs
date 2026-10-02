//! The `validate` and `schemas` commands: glue between argument parsing, [`crate::schema`], a cluster and
//! what gets printed.

use crate::cli::SchemasCmd;
use crate::render::{ResourceScope, Selector, render_in_memory};
use crate::repl::{error, ok, warn};
use crate::schema::{
    CrdSchema, Issue, Outcome, SchemaSet, Source, TEMPLATED_CRDS, cache_dir, load,
};
use anyhow::{Context as _, Result, bail, ensure};
use quench_cli::prelude::{Tone, print_status};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The API groups Kubernetes itself serves. A kind from one of these is not a custom resource, so having no
/// schema for it is expected rather than something `schemas fetch` could put right.
const BUILT_IN_GROUPS: [&str; 18] = [
    "",
    "apps",
    "batch",
    "autoscaling",
    "policy",
    "networking.k8s.io",
    "rbac.authorization.k8s.io",
    "storage.k8s.io",
    "scheduling.k8s.io",
    "certificates.k8s.io",
    "apiextensions.k8s.io",
    "apiregistration.k8s.io",
    "admissionregistration.k8s.io",
    "node.k8s.io",
    "discovery.k8s.io",
    "coordination.k8s.io",
    "events.k8s.io",
    "metrics.k8s.io",
];

/// Whether an `apiVersion` belongs to a group Kubernetes itself serves.
#[must_use]
pub fn is_built_in(api_version: &str) -> bool {
    let group = api_version.split_once('/').map_or("", |(group, _)| group);
    BUILT_IN_GROUPS.contains(&group)
}

/// What checking a set of documents found.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// Documents that had a schema and satisfied it.
    pub valid: usize,
    /// Built-in kinds, which are not checked, with how many documents each.
    pub built_in: BTreeMap<String, usize>,
    /// Kinds that look like custom resources but have no schema held, with how many documents each.
    pub skipped: BTreeMap<String, usize>,
    /// Documents that failed, as `kind/name` with what was wrong.
    pub failures: Vec<(String, Vec<Issue>)>,
}

/// `kind/name`, lowercase kind - how riveter names a resource everywhere else.
#[must_use]
pub fn label(document: &Value) -> String {
    let kind = document["kind"].as_str().unwrap_or("?").to_lowercase();
    let name = document["metadata"]["name"].as_str().unwrap_or("?");
    format!("{kind}/{name}")
}

/// Checks each document against `set`.
#[must_use]
pub fn check_documents(set: &SchemaSet, documents: &[Value]) -> Report {
    let mut report = Report::default();
    for document in documents {
        match set.check(document) {
            Outcome::Valid => report.valid += 1,
            Outcome::Skipped => {
                let kind = document["kind"].as_str().unwrap_or("?").to_lowercase();
                let built_in = is_built_in(document["apiVersion"].as_str().unwrap_or_default());
                let bucket = if built_in {
                    &mut report.built_in
                } else {
                    &mut report.skipped
                };
                *bucket.entry(kind).or_default() += 1;
            }
            Outcome::Invalid(issues) => report.failures.push((label(document), issues)),
        }
    }
    report
}

/// The documents in YAML text: every document of a multi-document file, with a `kind: List` (what
/// `kubectl get -o yaml` prints for several objects) flattened into its items.
pub fn parse_documents(text: &str) -> Result<Vec<Value>> {
    let mut documents = Vec::new();
    for (index, doc) in serde_yaml::Deserializer::from_str(text).enumerate() {
        let value: Value = serde::Deserialize::deserialize(doc)
            .with_context(|| format!("document {} is not valid YAML", index + 1))?;
        match value {
            Value::Null => {}
            Value::Object(ref map)
                if map.get("kind").and_then(Value::as_str) == Some("List")
                    || map.get("items").is_some_and(Value::is_array)
                        && map
                            .get("kind")
                            .and_then(Value::as_str)
                            .is_some_and(|k| k.ends_with("List")) =>
            {
                if let Some(items) = map.get("items").and_then(Value::as_array) {
                    documents.extend(items.iter().cloned());
                }
            }
            other => documents.push(other),
        }
    }
    Ok(documents)
}

fn print_report(report: &Report) {
    for (resource, issues) in &report.failures {
        for issue in issues {
            error(&format!("{resource}: {issue}"));
        }
    }
    if report.valid > 0 {
        ok(&format!(
            "{} resource(s) satisfy their schema",
            report.valid
        ));
    }
    if !report.built_in.is_empty() {
        print_status(
            Tone::Info,
            "info",
            &format!(
                "built-in kinds are not checked: {}",
                counts(&report.built_in)
            ),
        );
    }
    if !report.skipped.is_empty() {
        warn(&format!(
            "not checked, no schema held for: {} - `riveter schemas fetch --crd <name>` (or `--all`) reads them from a cluster",
            counts(&report.skipped)
        ));
    }
}

/// `deployment x3, service x2`.
fn counts(kinds: &BTreeMap<String, usize>) -> String {
    kinds
        .iter()
        .map(|(kind, count)| format!("{kind} x{count}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn outcome(report: &Report) -> Result<()> {
    ensure!(
        report.failures.is_empty(),
        "{} resource(s) failed validation",
        report.failures.len()
    );
    Ok(())
}

/// `riveter validate`, for an environment's rendered resources.
pub fn validate_env(env: &str, scope: ResourceScope, selector: &Selector) -> Result<()> {
    let set = load()?;
    let rendered = render_in_memory(env, scope, selector)?;

    let mut documents = Vec::with_capacity(rendered.len());
    for (resource, yaml) in &rendered {
        let mut parsed = parse_documents(yaml)
            .with_context(|| format!("could not read the rendered {resource}"))?;
        documents.append(&mut parsed);
    }

    let report = check_documents(&set, &documents);
    print_report(&report);
    outcome(&report)
}

/// `riveter validate --file`, for documents in files (`-` is standard input) rather than an overlay.
pub fn validate_files(paths: &[PathBuf]) -> Result<()> {
    let set = load()?;
    let mut documents = Vec::new();
    for path in paths {
        let text = if path == Path::new("-") {
            let mut text = String::new();
            std::io::Read::read_to_string(&mut std::io::stdin(), &mut text)?;
            text
        } else {
            std::fs::read_to_string(path)
                .with_context(|| format!("failed to read {}", path.display()))?
        };
        documents.extend(parse_documents(&text).with_context(|| format!("in {}", path.display()))?);
    }

    let report = check_documents(&set, &documents);
    print_report(&report);
    outcome(&report)
}

/// The CRDs in `kubectl get crd -o json` output: a single object for one name, a `List` for several.
pub fn crds_from_kubectl(json: &str) -> Result<Vec<Value>> {
    if json.trim().is_empty() {
        return Ok(Vec::new());
    }
    let value: Value = serde_json::from_str(json).context("kubectl did not print JSON")?;
    Ok(match value["items"].as_array() {
        Some(items) => items.clone(),
        None if value["kind"] == "CustomResourceDefinition" => vec![value],
        None => Vec::new(),
    })
}

/// What `schemas fetch` was asked for.
#[derive(Debug, Default)]
pub struct FetchRequest {
    /// CRD names (`certificates.cert-manager.io`); empty means the kinds riveter has templates for.
    pub crds: Vec<String>,
    /// Every CRD in the cluster.
    pub all: bool,
    /// A kubectl context other than the current one.
    pub context: Option<String>,
    /// Where to write, instead of the cache directory.
    pub output: Option<PathBuf>,
}

/// `riveter schemas fetch`.
pub fn fetch(request: &FetchRequest) -> Result<Vec<CrdSchema>> {
    let wanted: Vec<String> = if request.all {
        Vec::new()
    } else if request.crds.is_empty() {
        TEMPLATED_CRDS
            .iter()
            .map(|(group, plural)| format!("{plural}.{group}"))
            .collect()
    } else {
        request.crds.clone()
    };

    let mut command = Command::new("kubectl");
    if let Some(context) = &request.context {
        command.args(["--context", context]);
    }
    command.args(["get", "crd"]).args(&wanted);
    command.args(["-o", "json", "--ignore-not-found"]);
    let out = command
        .output()
        .context("could not run kubectl - schemas are read from a cluster's CRDs")?;
    ensure!(
        out.status.success(),
        "kubectl could not list the CRDs:\n{}",
        String::from_utf8_lossy(&out.stderr).trim()
    );

    let crds = crds_from_kubectl(&String::from_utf8_lossy(&out.stdout))?;
    let mut schemas = Vec::new();
    for crd in &crds {
        match CrdSchema::from_crd(crd) {
            Ok(schema) => schemas.push(schema),
            Err(err) => warn(&format!(
                "skipped {}: {err:#}",
                crd["metadata"]["name"].as_str().unwrap_or("a CRD")
            )),
        }
    }
    for name in &wanted {
        if !schemas.iter().any(|s| &s.crd_name() == name) {
            warn(&format!("{name} is not installed in this cluster"));
        }
    }
    ensure!(
        !schemas.is_empty(),
        "the cluster returned no CRD with a schema"
    );

    let dir = match &request.output {
        Some(dir) => dir.clone(),
        None => cache_dir()?,
    };
    std::fs::create_dir_all(&dir).with_context(|| format!("failed to create {}", dir.display()))?;
    for schema in &schemas {
        let path = dir.join(schema.file_name());
        let mut text = serde_json::to_string(schema)?;
        text.push('\n');
        std::fs::write(&path, text)
            .with_context(|| format!("failed to write {}", path.display()))?;
    }
    Ok(schemas)
}

/// `riveter schemas ...`.
pub fn schemas_command(cmd: &SchemasCmd) -> Result<()> {
    match cmd {
        SchemasCmd::List => {
            let set = load()?;
            let rows = set.list();
            ensure!(!rows.is_empty(), "no schemas held");
            for (schema, source) in rows {
                let versions: Vec<&str> = schema.versions.keys().map(String::as_str).collect();
                let cel = if schema.cel_rules > 0 {
                    format!("  ({} CEL rule(s) not evaluated)", schema.cel_rules)
                } else {
                    String::new()
                };
                println!(
                    "  {:<28} {:<16} {:<10} {:<8}{cel}",
                    schema.group,
                    schema.kind,
                    versions.join(","),
                    match source {
                        Source::Bundled => "bundled",
                        Source::Cache => "cache",
                    },
                );
            }
            Ok(())
        }
        SchemasCmd::Fetch {
            crd,
            all,
            context,
            output,
        } => {
            if *all && !crd.is_empty() {
                bail!("--all and --crd name the same thing two ways; use one");
            }
            let schemas = fetch(&FetchRequest {
                crds: crd.clone(),
                all: *all,
                context: context.clone(),
                output: output.clone(),
            })?;
            let target = match output {
                Some(dir) => dir.clone(),
                None => cache_dir()?,
            };
            ok(&format!(
                "fetched {} schema(s) into {}: {}",
                schemas.len(),
                target.display(),
                schemas
                    .iter()
                    .map(|s| format!("{}/{}", s.group, s.kind))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            Ok(())
        }
    }
}
