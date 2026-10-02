//! Offline validation of custom resources against the `OpenAPI` schema their CRD publishes.
//!
//! A CRD's `openAPIV3Schema` is almost JSON Schema, and is what the API server checks a custom resource
//! against, so a typo in a `Certificate` or an `IngressRoute` is caught by `kubectl apply` - after the
//! rest of an overlay has already been applied. Validating here, against a schema taken from the CRD
//! once, finds it before anything is sent.
//!
//! The schema is *normalised* before use, because the API server reads a few things differently from JSON
//! Schema: see [`normalize`]. Nothing in this module talks to a cluster; [`crate::schema_cmd`] does that.

use anyhow::{Context as _, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::path::Path;

/// Where a schema came from, for `schemas list`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// Compiled into riveter.
    Bundled,
    /// Fetched into the cache directory, which takes precedence.
    Cache,
}

/// One CRD's schemas, as stored on disk and in the binary: every served version, descriptions stripped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrdSchema {
    /// The API group, e.g. `cert-manager.io`.
    pub group: String,
    /// The kind, e.g. `Certificate`.
    pub kind: String,
    /// The plural resource name, e.g. `certificates`.
    pub plural: String,
    /// `Namespaced` or `Cluster`.
    pub scope: String,
    /// `x-kubernetes-validations` (CEL) rules the schema carries, which this validator cannot evaluate.
    #[serde(default)]
    pub cel_rules: usize,
    /// The `openAPIV3Schema` of each served version.
    pub versions: BTreeMap<String, Value>,
}

impl CrdSchema {
    /// The name the CRD is known by in the cluster: `<plural>.<group>`.
    #[must_use]
    pub fn crd_name(&self) -> String {
        format!("{}.{}", self.plural, self.group)
    }

    /// The file name a schema is stored under.
    #[must_use]
    pub fn file_name(&self) -> String {
        format!("{}_{}.json", self.group, self.plural)
    }

    /// Reads a `CustomResourceDefinition` as `kubectl get crd -o json` prints it.
    pub fn from_crd(crd: &Value) -> Result<Self> {
        let spec = &crd["spec"];
        let text = |value: &Value, what: &str| -> Result<String> {
            value
                .as_str()
                .map(str::to_string)
                .with_context(|| format!("the CRD has no {what}"))
        };

        let mut cel_rules = 0;
        let mut versions = BTreeMap::new();
        for version in spec["versions"].as_array().into_iter().flatten() {
            if version["served"].as_bool() != Some(true) {
                continue;
            }
            let name = text(&version["name"], "version name")?;
            let Some(schema) = version["schema"].get("openAPIV3Schema") else {
                continue;
            };
            let mut schema = schema.clone();
            cel_rules += strip(&mut schema);
            versions.insert(name, schema);
        }

        let schema = Self {
            group: text(&spec["group"], "group")?,
            kind: text(&spec["names"]["kind"], "kind")?,
            plural: text(&spec["names"]["plural"], "plural name")?,
            scope: text(&spec["scope"], "scope")?,
            cel_rules,
            versions,
        };
        ensure!(
            !schema.versions.is_empty(),
            "{} serves no version with an OpenAPI schema",
            schema.crd_name()
        );
        Ok(schema)
    }
}

/// Removes what only a human reads, so a stored schema is a fraction of the CRD's size, and returns how
/// many CEL rules (`x-kubernetes-validations`) it held.
///
/// Descriptions are most of a large CRD: Gateway API's `HTTPRoute` is several hundred kilobytes with
/// them and a small fraction without.
///
/// Walks *schemas*, not every object in the document. A field can be called `default`, `title` or
/// `description` - those names sit as keys of a `properties` map, where they are fields and must stay -
/// so only a schema node's own annotation keywords are removed, and `properties` is entered by its values.
fn strip(node: &mut Value) -> usize {
    let Value::Object(map) = node else {
        return 0;
    };
    let mut rules = 0;

    for key in ["description", "example", "externalDocs", "title", "default"] {
        map.remove(key);
    }
    if let Some(Value::Array(list)) = map.remove("x-kubernetes-validations") {
        rules += list.len();
    }

    for key in ["properties", "patternProperties", "$defs", "definitions"] {
        if let Some(Value::Object(children)) = map.get_mut(key) {
            for child in children.values_mut() {
                rules += strip(child);
            }
        }
    }
    for key in ["items", "additionalProperties", "contains", "not"] {
        if let Some(child) = map.get_mut(key) {
            rules += strip(child);
        }
    }
    for key in ["oneOf", "anyOf", "allOf"] {
        if let Some(Value::Array(branches)) = map.get_mut(key) {
            for branch in branches {
                rules += strip(branch);
            }
        }
    }
    rules
}

/// Turns a CRD schema into JSON Schema that means what the API server means by it.
///
/// - **Unknown fields are errors**, as `kubectl apply` reports them (strict field validation): an object
///   that lists its `properties` gets `additionalProperties: false`, unless it says
///   `x-kubernetes-preserve-unknown-fields` or already has an `additionalProperties`. Not inside
///   `oneOf`/`anyOf`/`allOf`/`not`, where a branch lists only the properties it constrains.
/// - `x-kubernetes-int-or-string` becomes "integer or string".
/// - `nullable: true` admits `null`.
/// - Keywords JSON Schema does not have (`x-kubernetes-*`, `OpenAPI`'s boolean `exclusiveMinimum`, `format`)
///   are removed rather than left to be rejected.
#[must_use]
pub fn normalize(schema: &Value) -> Value {
    let mut out = schema.clone();
    normalize_node(&mut out, true);
    out
}

fn normalize_node(node: &mut Value, closed_allowed: bool) {
    let Value::Object(map) = node else {
        if let Value::Array(items) = node {
            for item in items {
                normalize_node(item, closed_allowed);
            }
        }
        return;
    };

    let int_or_string = map.get("x-kubernetes-int-or-string") == Some(&Value::Bool(true));
    let preserve = map.get("x-kubernetes-preserve-unknown-fields") == Some(&Value::Bool(true));
    let nullable = map.get("nullable") == Some(&Value::Bool(true));

    let doomed: Vec<String> = map
        .keys()
        .filter(|k| k.starts_with("x-kubernetes-") || *k == "nullable" || *k == "format")
        .cloned()
        .collect();
    for key in doomed {
        map.remove(&key);
    }
    // OpenAPI 3.0 spells these as booleans qualifying `minimum`/`maximum`; JSON Schema wants numbers.
    for key in ["exclusiveMinimum", "exclusiveMaximum"] {
        if map.get(key).is_some_and(Value::is_boolean) {
            map.remove(key);
        }
    }

    // Recurse into subschemas, remembering whether closing objects is meaningful there.
    let has_properties = map.get("properties").is_some_and(Value::is_object);
    for key in ["properties", "patternProperties", "$defs", "definitions"] {
        if let Some(Value::Object(children)) = map.get_mut(key) {
            for child in children.values_mut() {
                normalize_node(child, closed_allowed);
            }
        }
    }
    for key in ["items", "additionalProperties", "contains"] {
        if let Some(child) = map.get_mut(key) {
            normalize_node(child, closed_allowed);
        }
    }
    for key in ["oneOf", "anyOf", "allOf"] {
        if let Some(Value::Array(branches)) = map.get_mut(key) {
            for branch in branches {
                normalize_node(branch, false);
            }
        }
    }
    if let Some(child) = map.get_mut("not") {
        normalize_node(child, false);
    }

    if closed_allowed && has_properties && !preserve && !map.contains_key("additionalProperties") {
        map.insert("additionalProperties".to_string(), Value::Bool(false));
    }

    if int_or_string {
        map.remove("type");
        map.insert(
            "anyOf".to_string(),
            json!([{ "type": "integer" }, { "type": "string" }]),
        );
    }

    if nullable {
        if let Some(Value::String(kind)) = map.get("type").cloned() {
            map.insert("type".to_string(), json!([kind, "null"]));
        } else {
            // No single type to widen: accept null alongside whatever else is allowed.
            let rest = Value::Object(std::mem::take(map));
            map.insert("anyOf".to_string(), json!([rest, { "type": "null" }]));
        }
    }
}

/// One thing wrong with a document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    /// Where, as `spec.dnsNames[0]`; empty for the document itself.
    pub path: String,
    /// What is wrong.
    pub message: String,
    /// A field of the same object this might have been meant as, for an unknown one.
    pub suggestion: Option<String>,
}

impl std::fmt::Display for Issue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if !self.path.is_empty() {
            write!(f, "{}: ", self.path)?;
        }
        write!(f, "{}", self.message)?;
        if let Some(suggestion) = &self.suggestion {
            write!(f, " (did you mean `{suggestion}`?)")?;
        }
        Ok(())
    }
}

/// What checking one document found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// No schema is known for its `apiVersion`/`kind`, so nothing could be checked.
    Skipped,
    /// It satisfies its schema.
    Valid,
    /// It does not.
    Invalid(Vec<Issue>),
}

/// A kind at one version: group, kind, version.
type VersionKey = (String, String, String);

struct Entry {
    schema: CrdSchema,
    source: Source,
}

/// The schemas riveter knows: those compiled in, overlaid by those fetched into a cache directory.
#[derive(Default)]
pub struct SchemaSet {
    entries: HashMap<(String, String), Entry>,
    compiled: std::cell::RefCell<HashMap<VersionKey, Option<std::rc::Rc<jsonschema::Validator>>>>,
}

impl std::fmt::Debug for SchemaSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SchemaSet")
            .field("kinds", &self.entries.len())
            .finish_non_exhaustive()
    }
}

impl SchemaSet {
    /// An empty set; nothing validates against it.
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Adds a schema, replacing one of the same group and kind - which is how a fetched schema beats a
    /// bundled one.
    pub fn insert(&mut self, schema: CrdSchema, source: Source) {
        self.entries.insert(
            (schema.group.clone(), schema.kind.clone()),
            Entry { schema, source },
        );
    }

    /// Adds every `*.json` schema in `dir`. A missing directory is not an error (nothing fetched yet);
    /// a file that is not a schema is, because silently ignoring it would leave a kind unchecked.
    pub fn load_dir(&mut self, dir: &Path, source: Source) -> Result<()> {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Ok(());
        };
        let mut paths: Vec<_> = entries
            .filter_map(std::result::Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "json"))
            .collect();
        paths.sort();
        for path in paths {
            let text = std::fs::read_to_string(&path)
                .with_context(|| format!("failed to read {}", path.display()))?;
            let schema: CrdSchema = serde_json::from_str(&text)
                .with_context(|| format!("{} is not a riveter schema file", path.display()))?;
            self.insert(schema, source);
        }
        Ok(())
    }

    /// Every schema held, sorted by group then kind.
    #[must_use]
    pub fn list(&self) -> Vec<(&CrdSchema, Source)> {
        let mut all: Vec<_> = self
            .entries
            .values()
            .map(|e| (&e.schema, e.source))
            .collect();
        all.sort_by(|a, b| (&a.0.group, &a.0.kind).cmp(&(&b.0.group, &b.0.kind)));
        all
    }

    /// The schema for a group and kind, if one is held.
    #[must_use]
    pub fn get(&self, group: &str, kind: &str) -> Option<&CrdSchema> {
        self.entries
            .get(&(group.to_string(), kind.to_string()))
            .map(|e| &e.schema)
    }

    fn validator(
        &self,
        group: &str,
        kind: &str,
        version: &str,
    ) -> Option<std::rc::Rc<jsonschema::Validator>> {
        let key = (group.to_string(), kind.to_string(), version.to_string());
        if let Some(found) = self.compiled.borrow().get(&key) {
            return found.clone();
        }
        let built = self
            .get(group, kind)
            .and_then(|s| s.versions.get(version))
            .and_then(|schema| jsonschema::validator_for(&normalize(schema)).ok())
            .map(std::rc::Rc::new);
        self.compiled.borrow_mut().insert(key, built.clone());
        built
    }

    /// Checks one document against the schema for its `apiVersion` and `kind`.
    #[must_use]
    pub fn check(&self, document: &Value) -> Outcome {
        let (Some(api_version), Some(kind)) =
            (document["apiVersion"].as_str(), document["kind"].as_str())
        else {
            return Outcome::Skipped;
        };
        let (group, version) = api_version.split_once('/').unwrap_or(("", api_version));

        let Some(schema) = self.get(group, kind) else {
            return Outcome::Skipped;
        };
        let Some(validator) = self.validator(group, kind, version) else {
            let served: Vec<&str> = schema.versions.keys().map(String::as_str).collect();
            return Outcome::Invalid(vec![Issue {
                path: "apiVersion".to_string(),
                message: format!(
                    "{api_version} is not a version of {} that the CRD serves (it serves: {})",
                    schema.crd_name(),
                    served.join(", ")
                ),
                suggestion: None,
            }]);
        };

        let normalized = schema.versions.get(version).map_or(Value::Null, normalize);

        let mut issues: Vec<Issue> = validator
            .iter_errors(document)
            .map(|error| issue_from(&error, &normalized))
            .collect();
        issues.sort_by(|a, b| a.path.cmp(&b.path).then_with(|| a.message.cmp(&b.message)));
        issues.dedup();

        if issues.is_empty() {
            Outcome::Valid
        } else {
            Outcome::Invalid(issues)
        }
    }
}

/// `/spec/dnsNames/0` as `spec.dnsNames[0]`.
#[must_use]
pub fn display_path(pointer: &str) -> String {
    let mut out = String::new();
    for segment in pointer.split('/').filter(|s| !s.is_empty()) {
        let segment = segment.replace("~1", "/").replace("~0", "~");
        if segment.chars().all(|c| c.is_ascii_digit()) {
            let _ = write!(out, "[{segment}]");
        } else {
            if !out.is_empty() {
                out.push('.');
            }
            out.push_str(&segment);
        }
    }
    out
}

fn issue_from(error: &jsonschema::ValidationError<'_>, root: &Value) -> Issue {
    let pointer = error.instance_path().to_string();
    let mut message = error.to_string();
    let mut suggestion = None;

    if let jsonschema::error::ValidationErrorKind::AdditionalProperties { unexpected } =
        error.kind()
    {
        let names = unexpected.join("`, `");
        message = format!("unknown field `{names}`");
        if let [only] = unexpected.as_slice() {
            suggestion = nearest_property(root, &pointer, only);
        }
    }

    Issue {
        path: display_path(&pointer),
        message,
        suggestion,
    }
}

/// The property of the object at `pointer` closest in spelling to `unknown`, if one is close enough to
/// be a likely typo.
fn nearest_property(root: &Value, pointer: &str, unknown: &str) -> Option<String> {
    let mut node = root;
    for segment in pointer.split('/').filter(|s| !s.is_empty()) {
        let segment = segment.replace("~1", "/").replace("~0", "~");
        node = if segment.chars().all(|c| c.is_ascii_digit()) {
            node.get("items")?
        } else {
            node.get("properties")
                .and_then(|p| p.get(&segment))
                .or_else(|| node.get("additionalProperties"))?
        };
    }

    let limit = (unknown.chars().count() / 3).clamp(1, 3);
    node.get("properties")?
        .as_object()?
        .keys()
        .map(|candidate| (distance(unknown, candidate), candidate))
        .filter(|(d, _)| *d <= limit)
        .min_by_key(|(d, candidate)| (*d, candidate.len()))
        .map(|(_, candidate)| candidate.clone())
}

/// Edit distance between two names, case-insensitively, counting a swap of two neighbouring letters as
/// one edit (optimal string alignment). Transposition is the commonest typo, and plain Levenshtein
/// charges it two, which puts `mathc` outside the reach of `match`.
fn distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.to_lowercase().chars().collect();
    let b: Vec<char> = b.to_lowercase().chars().collect();
    let mut grid = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in grid.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in grid[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            let mut best = (grid[i - 1][j] + 1)
                .min(grid[i][j - 1] + 1)
                .min(grid[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                best = best.min(grid[i - 2][j - 2] + 1);
            }
            grid[i][j] = best;
        }
    }
    grid[a.len()][b.len()]
}

/// The CRDs riveter has templates for, as `(group, plural)`. These are what the bundle holds and what
/// `schemas fetch` fetches by default.
pub const TEMPLATED_CRDS: [(&str, &str); 7] = [
    ("cert-manager.io", "certificates"),
    ("cert-manager.io", "clusterissuers"),
    ("cert-manager.io", "issuers"),
    ("gateway.networking.k8s.io", "gateways"),
    ("gateway.networking.k8s.io", "httproutes"),
    ("traefik.io", "ingressroutes"),
    ("traefik.io", "middlewares"),
];

/// The schemas compiled into riveter, as the files `schemas fetch --output` writes.
const BUNDLED: [&str; 7] = [
    include_str!("schemas/cert-manager.io_certificates.json"),
    include_str!("schemas/cert-manager.io_clusterissuers.json"),
    include_str!("schemas/cert-manager.io_issuers.json"),
    include_str!("schemas/gateway.networking.k8s.io_gateways.json"),
    include_str!("schemas/gateway.networking.k8s.io_httproutes.json"),
    include_str!("schemas/traefik.io_ingressroutes.json"),
    include_str!("schemas/traefik.io_middlewares.json"),
];

/// The bundled schemas, ready to extend with a cache.
pub fn bundled() -> Result<SchemaSet> {
    let mut set = SchemaSet::empty();
    for text in BUNDLED {
        let schema: CrdSchema =
            serde_json::from_str(text).context("a bundled schema is corrupt")?;
        set.insert(schema, Source::Bundled);
    }
    Ok(set)
}

/// Where fetched schemas are cached: `$RIVETER_SCHEMA_DIR`, else `$XDG_CACHE_HOME/riveter/schemas`, else
/// `~/.cache/riveter/schemas`.
pub fn cache_dir() -> Result<std::path::PathBuf> {
    if let Some(dir) = std::env::var_os("RIVETER_SCHEMA_DIR").filter(|d| !d.is_empty()) {
        return Ok(dir.into());
    }
    if let Some(dir) = std::env::var_os("XDG_CACHE_HOME").filter(|d| !d.is_empty()) {
        return Ok(std::path::PathBuf::from(dir).join("riveter/schemas"));
    }
    match std::env::var_os("HOME").filter(|d| !d.is_empty()) {
        Some(home) => Ok(std::path::PathBuf::from(home).join(".cache/riveter/schemas")),
        None => bail!("cannot find a cache directory: set RIVETER_SCHEMA_DIR"),
    }
}

/// Bundled schemas, overlaid by whatever is in the cache directory.
pub fn load() -> Result<SchemaSet> {
    let mut set = bundled()?;
    set.load_dir(&cache_dir()?, Source::Cache)?;
    Ok(set)
}
