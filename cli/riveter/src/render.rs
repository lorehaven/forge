use crate::env::{OUTPUT_DIR, OVERLAY_DIR, manifest_path};
use anyhow::Context;
use minijinja::{Environment, Value, context};
use regex::Regex;
use serde_yaml::Value as YamlValue;
use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceScope {
    Mutable,
    Immutable,
    All,
}

impl fmt::Display for ResourceScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Mutable => "mutable",
            Self::Immutable => "immutable",
            Self::All => "all",
        })
    }
}

/// One resource in an overlay, identified the way a user refers to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceRef {
    pub kind: String,
    pub name: String,
    pub immutable: bool,
}

impl ResourceRef {
    #[must_use]
    pub const fn in_scope(&self, scope: ResourceScope) -> bool {
        match scope {
            ResourceScope::All => true,
            ResourceScope::Mutable => !self.immutable,
            ResourceScope::Immutable => self.immutable,
        }
    }
}

impl fmt::Display for ResourceRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.kind, self.name)
    }
}

/// A `kind[/name]` pattern. Both halves accept `*` and `?` wildcards.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Target {
    kind: String,
    name: String,
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.kind, self.name)
    }
}

/// Which resources of an overlay a command should act on. An empty selector
/// means "everything in scope", i.e. the behaviour of a bare `apply`.
#[derive(Debug, Clone, Default)]
pub struct Selector {
    targets: Vec<Target>,
}

impl Selector {
    /// Parses `kind`, `kind/name` or `*/name` patterns. `kind` alone is
    /// equivalent to `kind/*`.
    pub fn parse<S: AsRef<str>>(targets: &[S]) -> anyhow::Result<Self> {
        let mut parsed = Vec::with_capacity(targets.len());

        for raw in targets {
            let raw = raw.as_ref().trim();
            anyhow::ensure!(!raw.is_empty(), "empty target (expected kind[/name])");

            let mut parts = raw.split('/');
            let kind = parts.next().unwrap_or("");
            let name = parts.next();
            anyhow::ensure!(
                parts.next().is_none(),
                "invalid target `{raw}` (expected kind[/name])"
            );

            parsed.push(Target {
                kind: blank_to_star(kind).to_lowercase(),
                name: blank_to_star(name.unwrap_or("*")).to_lowercase(),
            });
        }

        Ok(Self { targets: parsed })
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.targets.is_empty()
    }

    #[must_use]
    pub fn matches(&self, kind: &str, name: &str) -> bool {
        self.targets.is_empty() || self.targets.iter().any(|t| t.matches(kind, name))
    }
}

impl Target {
    fn matches(&self, kind: &str, name: &str) -> bool {
        self.kind_matches(kind) && wildcard_match(&self.name, &name.to_lowercase())
    }

    /// Kinds match on the literal spelling or on the canonical one, so both
    /// `sts/pg` and `StatefulSet/pg` select a `kind: statefulset` resource.
    fn kind_matches(&self, kind: &str) -> bool {
        let kind = kind.to_lowercase();

        wildcard_match(&self.kind, &kind)
            || wildcard_match(&canonical_kind(&self.kind), &canonical_kind(&kind))
    }
}

const fn blank_to_star(s: &str) -> &str {
    if s.is_empty() { "*" } else { s }
}

/// Glob matching over `*` (any run) and `?` (one character), iterative so a
/// pathological pattern cannot blow the stack.
fn wildcard_match(pattern: &str, value: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let value: Vec<char> = value.chars().collect();

    let (mut p, mut v) = (0usize, 0usize);
    let mut star: Option<(usize, usize)> = None;

    while v < value.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == value[v]) {
            p += 1;
            v += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star = Some((p, v));
            p += 1;
        } else if let Some((sp, sv)) = star {
            p = sp + 1;
            v = sv + 1;
            star = Some((sp, sv + 1));
        } else {
            return false;
        }
    }

    pattern[p..].iter().all(|c| *c == '*')
}

#[derive(Debug, Clone)]
pub struct RenderedManifest {
    pub path: String,
    pub resource_count: usize,
    /// The resources actually written, in manifest order.
    pub selected: Vec<ResourceRef>,
    /// The kubectl context the overlay binds itself to, if it declares one.
    pub kube_context: Option<String>,
    /// Resources the overlay declares that this scope left out, so a command
    /// can say what it is not touching.
    pub skipped_out_of_scope: Vec<ResourceRef>,
    /// The namespace the overlay's resources are rendered into.
    pub namespace: Option<String>,
    /// Whether this render includes the overlay's own `namespace` resource —
    /// i.e. whether applying it would create the namespace.
    pub creates_namespace: bool,
}

pub fn generate_manifests(env_name: &str) -> anyhow::Result<String> {
    Ok(generate_manifests_with_scope(env_name, ResourceScope::All)?.path)
}

pub fn generate_manifests_with_scope(
    env_name: &str,
    scope: ResourceScope,
) -> anyhow::Result<RenderedManifest> {
    generate_manifests_selected(env_name, scope, &Selector::default())
}

/// Lists every resource declared by an overlay, without rendering templates.
pub fn list_resources(env_name: &str) -> anyhow::Result<Vec<ResourceRef>> {
    let data = render_overlay(env_name)?;

    resource_refs(&data)
}

pub fn generate_manifests_selected(
    env_name: &str,
    scope: ResourceScope,
    selector: &Selector,
) -> anyhow::Result<RenderedManifest> {
    let data = render_overlay(env_name)?;

    if !selector.is_empty() {
        let all = resource_refs(&data)?;
        ensure_targets_match(selector, &all, scope)?;
    }

    let rendered = render_resources(env_name, &data, scope, selector)?;

    fs::create_dir_all(OUTPUT_DIR)?;
    ignore_output_dir()?;
    let path = if selector.is_empty() {
        scoped_manifest_path(env_name, scope)
    } else {
        format!("{OUTPUT_DIR}/{env_name}-manifests.selection.yaml")
    };

    let contents = join_manifests(&rendered);
    let sensitive = rendered
        .iter()
        .any(|(r, yaml)| is_secret_resource(&r.kind, yaml));
    write_manifest(&path, &contents, sensitive)?;

    let selected: Vec<ResourceRef> = rendered.into_iter().map(|(r, _)| r).collect();
    let skipped_out_of_scope = resource_refs(&data)?
        .into_iter()
        .filter(|r| !selected.contains(r))
        .collect();
    let creates_namespace = selected
        .iter()
        .any(|r| r.kind.eq_ignore_ascii_case("namespace"));

    Ok(RenderedManifest {
        path,
        resource_count: selected.len(),
        selected,
        kube_context: kube_context(&data)?,
        skipped_out_of_scope,
        namespace: data["namespace_name"]
            .as_str()
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .map(ToString::to_string),
        creates_namespace,
    })
}

/// Keeps rendered manifests out of version control.
///
/// Restricting a Secret-bearing manifest to its owner protects it from other
/// users on the machine and does nothing at all against `git add -A`; these
/// files are build output, so the safe default is for git never to see them.
/// An existing `.gitignore` is left alone.
fn ignore_output_dir() -> anyhow::Result<()> {
    let path = format!("{OUTPUT_DIR}/.gitignore");
    if Path::new(&path).exists() {
        return Ok(());
    }

    fs::write(
        &path,
        "# Written by riveter: rendered manifests are build output, and may\n\
         # contain plaintext Secrets.\n*\n",
    )
    .with_context(|| format!("failed to write {path}"))
}

/// Whether a rendered resource carries Secret data — either because its `kind`
/// says so, or because a `raw` block declares one.
#[doc(hidden)]
#[must_use]
pub fn is_secret_resource(kind: &str, yaml: &str) -> bool {
    canonical_kind(&kind.to_lowercase()) == "secret"
        || yaml.lines().any(|line| line.trim() == "kind: Secret")
}

/// Writes a manifest, restricting it to its owner when it carries a Secret.
///
/// A rendered Secret sits on disk in plaintext; at the default mode every other
/// user on the machine can read it.
fn write_manifest(path: &str, contents: &str, sensitive: bool) -> anyhow::Result<()> {
    if sensitive {
        return write_owner_only(path, contents);
    }

    fs::write(path, contents).with_context(|| format!("failed to write {path}"))
}

#[cfg(unix)]
fn write_owner_only(path: &str, contents: &str) -> anyhow::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    // Create or truncate first and tighten the mode while the file is still
    // empty, so the secret is never briefly readable at a wider mode. `mode`
    // only applies to a file being created, so an already-present manifest
    // needs the explicit `set_permissions` as well.
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .with_context(|| format!("failed to open {path}"))?;

    file.set_permissions(fs::Permissions::from_mode(0o600))
        .with_context(|| format!("failed to restrict {path} to its owner"))?;
    file.write_all(contents.as_bytes())
        .with_context(|| format!("failed to write {path}"))?;

    Ok(())
}

#[cfg(not(unix))]
fn write_owner_only(path: &str, contents: &str) -> anyhow::Result<()> {
    fs::write(path, contents).with_context(|| format!("failed to write {path}"))
}

/// The kubectl context an overlay binds itself to via a top-level
/// `kube_context:`, so that `apply` cannot land on whichever cluster kubectl
/// happens to be pointing at.
#[doc(hidden)]
pub fn kube_context(data: &YamlValue) -> anyhow::Result<Option<String>> {
    match &data["kube_context"] {
        YamlValue::Null => Ok(None),
        YamlValue::String(raw) => {
            let context = raw.trim();
            anyhow::ensure!(
                !context.is_empty(),
                "kube_context is empty — name a context, or remove the key to use kubectl's current one"
            );
            Ok(Some(context.to_string()))
        }
        _ => anyhow::bail!("kube_context must be a string naming a kubectl context"),
    }
}

fn resource_refs(data: &YamlValue) -> anyhow::Result<Vec<ResourceRef>> {
    let resources = data["resources"]
        .as_sequence()
        .context("resources must be a list")?;

    resources
        .iter()
        .map(|res| {
            let kind = res["kind"].as_str().context("kind missing")?;

            // `namespace` takes its name from the overlay, not from the entry.
            let name = res["name"].as_str().unwrap_or_else(|| {
                if kind.eq_ignore_ascii_case("namespace") {
                    data["namespace_name"].as_str().unwrap_or_default()
                } else {
                    ""
                }
            });

            Ok(ResourceRef {
                kind: kind.to_string(),
                name: name.to_string(),
                immutable: resource_is_immutable(res),
            })
        })
        .collect()
}

/// Fails when a target selects nothing, so a typo cannot quietly turn into a
/// no-op apply.
fn ensure_targets_match(
    selector: &Selector,
    all: &[ResourceRef],
    scope: ResourceScope,
) -> anyhow::Result<()> {
    for target in &selector.targets {
        let matched: Vec<&ResourceRef> = all
            .iter()
            .filter(|r| target.matches(&r.kind, &r.name))
            .collect();

        anyhow::ensure!(
            !matched.is_empty(),
            "no resource matches `{target}`\n\navailable resources:\n{}",
            format_refs(all.iter())
        );

        anyhow::ensure!(
            matched.iter().any(|r| r.in_scope(scope)),
            "`{target}` matches only resources outside --scope {scope}:\n{}\n\n\
             pass `--scope all` to include them",
            format_refs(matched.into_iter())
        );
    }

    Ok(())
}

fn format_refs<'a>(refs: impl Iterator<Item = &'a ResourceRef>) -> String {
    refs.map(|r| format!("  {r}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_overlay(env_name: &str) -> anyhow::Result<YamlValue> {
    let (env_vars, env_source) = load_env(env_name)?;
    let overlay_src = fs::read_to_string(format!("{OVERLAY_DIR}/{env_name}/overlay.yaml"))?;

    overlay_data(env_name, &overlay_src, &env_vars, env_source.as_deref())
}

/// Renders an overlay held in memory the way [`generate_manifests_selected`]
/// renders one on disk, so the templates can be exercised against a fixture
/// without a working directory to set up.
#[doc(hidden)]
pub fn render_to_string<S: std::hash::BuildHasher>(
    env_name: &str,
    overlay_src: &str,
    env_vars: &HashMap<String, String, S>,
) -> anyhow::Result<String> {
    let data = overlay_data(env_name, overlay_src, env_vars, None)?;
    let rendered = render_resources(env_name, &data, ResourceScope::All, &Selector::default())?;

    Ok(join_manifests(&rendered))
}

/// The overlay pipeline from source text to data: Jinja, then YAML, then
/// `${VAR}` expansion, then the checks that apply to the document as a whole.
fn overlay_data<S: std::hash::BuildHasher>(
    env_name: &str,
    overlay_src: &str,
    env_vars: &HashMap<String, String, S>,
    env_source: Option<&str>,
) -> anyhow::Result<YamlValue> {
    let mut overlay_jinja = Environment::new();
    overlay_jinja.set_loader(minijinja::path_loader(OVERLAY_DIR));
    overlay_jinja.add_global("env", env_name);

    let rendered_overlay = overlay_jinja.render_str(overlay_src, Value::UNDEFINED)?;
    let mut data: YamlValue = serde_yaml::from_str(&rendered_overlay).map_err(|e| {
        let msg = format!("Failed to parse overlay.yaml after rendering: {e}");
        // Print with line numbers for easier debugging
        let with_lines = rendered_overlay
            .lines()
            .enumerate()
            .map(|(i, l)| format!("{:3} | {}", i + 1, l))
            .collect::<Vec<_>>()
            .join("\n");

        anyhow::anyhow!("{msg}\n\nRendered source:\n{with_lines}")
    })?;

    substitute_vars(&mut data, env_vars, env_source)?;
    ensure_defaults_is_a_mapping(&data)?;
    ensure_resources_are_addressable(&data)?;

    Ok(data)
}

/// Rejects resources that cannot be named or told apart.
///
/// Both failures are otherwise silent: a nameless resource renders
/// `metadata.name:` empty and is refused by the cluster with a far less
/// obvious message, and two entries sharing a `kind/name` both render, so the
/// second quietly overwrites the first on apply and no target can ever select
/// just one of them.
fn ensure_resources_are_addressable(data: &YamlValue) -> anyhow::Result<()> {
    let refs = resource_refs(data)?;

    let unnamed: Vec<String> = refs
        .iter()
        .enumerate()
        .filter(|(_, r)| r.name.trim().is_empty())
        .map(|(i, r)| format!("  resources[{i}] (kind: {})", r.kind))
        .collect();

    anyhow::ensure!(
        unnamed.is_empty(),
        "resource(s) with no name:\n{}\n\ngive each a `name:`{}",
        unnamed.join("\n"),
        if refs
            .iter()
            .any(|r| r.kind.eq_ignore_ascii_case("namespace"))
        {
            " — `namespace` takes its name from the overlay's `namespace_name`"
        } else {
            ""
        }
    );

    let mut seen: BTreeMap<(String, String), usize> = BTreeMap::new();
    let mut duplicates: Vec<String> = Vec::new();
    for r in &refs {
        let key = (
            canonical_kind(&r.kind.to_lowercase()),
            r.name.to_lowercase(),
        );
        *seen.entry(key).or_default() += 1;
    }
    for r in &refs {
        let key = (
            canonical_kind(&r.kind.to_lowercase()),
            r.name.to_lowercase(),
        );
        if seen.remove(&key).is_some_and(|count| count > 1) {
            duplicates.push(format!("  {r}"));
        }
    }

    anyhow::ensure!(
        duplicates.is_empty(),
        "resource(s) declared more than once:\n{}\n\n\
         each `kind/name` must be unique — the last one would win on apply",
        duplicates.join("\n")
    );

    Ok(())
}

/// Joins rendered resources into one multi-document manifest.
fn join_manifests(rendered: &[(ResourceRef, String)]) -> String {
    if rendered.is_empty() {
        return String::new();
    }

    let body = rendered
        .iter()
        .map(|(_, yaml)| yaml.as_str())
        .collect::<Vec<_>>()
        .join("\n---\n");

    strip_empty_lines(&(body + "\n"))
}

/// The overlay's `defaults:` block feeds the pod-based templates by attribute
/// lookup, which quietly yields nothing for a non-mapping — so a mistyped block
/// would silently drop every default it was meant to set.
fn ensure_defaults_is_a_mapping(data: &YamlValue) -> anyhow::Result<()> {
    anyhow::ensure!(
        matches!(&data["defaults"], YamlValue::Null | YamlValue::Mapping(_)),
        "defaults must be a mapping of fallback values, e.g.\n  \
         defaults:\n    service_account: my-app-sa"
    );

    Ok(())
}

fn render_resources(
    env_name: &str,
    data: &YamlValue,
    scope: ResourceScope,
    selector: &Selector,
) -> anyhow::Result<Vec<(ResourceRef, String)>> {
    let resources = data["resources"]
        .as_sequence()
        .context("resources must be a list")?;
    let refs = resource_refs(data)?;

    let mut tpl_env = Environment::new();
    load_embedded_templates(&mut tpl_env)?;
    tpl_env.add_global("env", env_name);
    tpl_env.add_filter("to_yaml", to_yaml);

    let mut out = Vec::new();
    for (res, res_ref) in resources.iter().zip(refs) {
        if !resource_in_scope(res, scope) || !selector.matches(&res_ref.kind, &res_ref.name) {
            continue;
        }

        let kind = res["kind"].as_str().context("kind missing")?;
        let tpl_name = template_name_for_kind(kind);

        let tpl = tpl_env
            .get_template(&tpl_name)
            .with_context(|| format!("template for kind `{kind}` not found or invalid. ensure `{tpl_name}` exists in embedded templates."))?;

        let y = tpl.render(context! {
            data => data,
            res => res,
            env => env_name,
        })?;
        out.push((res_ref, y.trim().to_string()));
    }
    Ok(out)
}

fn scoped_manifest_path(env: &str, scope: ResourceScope) -> String {
    match scope {
        ResourceScope::All => manifest_path(env),
        ResourceScope::Mutable => format!("{OUTPUT_DIR}/{env}-manifests.mutable.yaml"),
        ResourceScope::Immutable => format!("{OUTPUT_DIR}/{env}-manifests.immutable.yaml"),
    }
}

#[doc(hidden)]
#[must_use]
pub fn resource_in_scope(res: &YamlValue, scope: ResourceScope) -> bool {
    match scope {
        ResourceScope::All => true,
        ResourceScope::Mutable => !resource_is_immutable(res),
        ResourceScope::Immutable => resource_is_immutable(res),
    }
}

fn resource_is_immutable(res: &YamlValue) -> bool {
    if res["immutable"].as_bool().unwrap_or(false) {
        return true;
    }

    if let Some(lifecycle) = res["lifecycle"].as_str() {
        let lifecycle = lifecycle.trim();
        return lifecycle.eq_ignore_ascii_case("immutable")
            || lifecycle.eq_ignore_ascii_case("static");
    }

    false
}

/// Loads an environment's variables from its own `.env`, and only its own.
///
/// There is deliberately no fallback to a shared `.env`: one would let an
/// environment resolve a variable from a file belonging to a different
/// environment, quietly rendering (say) production with development's
/// credentials. A missing definition is better as the loud error
/// [`substitute_vars`] raises, which names this path.
///
/// Returns the path either way, so that error can point at the file that should
/// have defined the variable even when the file does not exist yet.
fn load_env(env: &str) -> anyhow::Result<(HashMap<String, String>, Option<String>)> {
    let path = format!("{OVERLAY_DIR}/{env}/.env");
    if !Path::new(&path).exists() {
        return Ok((HashMap::new(), Some(path)));
    }

    let env_content = fs::read_to_string(&path)?;
    let mut env_vars = HashMap::new();

    for line in env_content.lines() {
        let line = line.trim();
        // Skip comments and empty lines
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim().to_string();
            let mut value = value.trim().to_string();

            // Remove surrounding quotes if present
            if (value.starts_with('"') && value.ends_with('"'))
                || (value.starts_with('\'') && value.ends_with('\''))
            {
                value = value[1..value.len() - 1].to_string();
            }

            env_vars.insert(key, value);
        }
    }

    Ok((env_vars, Some(path)))
}

macro_rules! embedded_templates {
    ($($name:literal),* $(,)?) => {
        [$((concat!($name, ".yaml.j2"), include_str!(concat!("templates/", $name, ".yaml.j2")))),*]
    };
}

/// Every embedded template, keyed by file name. A resource `kind` maps to an
/// entry here by lowercasing it (see [`template_name_for_kind`]).
const TEMPLATES: &[(&str, &str)] = &embedded_templates![
    // shared macro library, imported by the templates below
    "_macros",
    // workloads
    "cronjob",
    "daemonset",
    "deployment",
    "job",
    "pod",
    "replicaset",
    "statefulset",
    // config & storage
    "configmap",
    "pv",
    "pvc",
    "secret",
    "storageclass",
    // networking
    "endpoints",
    "endpointslice",
    "gateway",
    "httproute",
    "ingress",
    "ingressclass",
    "ingressroute",
    "middleware",
    "networkpolicy",
    "service",
    // scaling, scheduling & availability
    "horizontalpodautoscaler",
    "poddisruptionbudget",
    "priorityclass",
    "runtimeclass",
    // policy & quota
    "limitrange",
    "namespace",
    "resourcequota",
    // rbac
    "clusterrole",
    "clusterrolebinding",
    "role",
    "rolebinding",
    "serviceaccount",
    // api extension
    "apiservice",
    "customresourcedefinition",
    "mutatingwebhookconfiguration",
    "validatingwebhookconfiguration",
    // cert-manager
    "certificate",
    "clusterissuer",
    "issuer",
    // escape hatch
    "raw",
];

/// Shorthand `kind` values accepted in overlays, mapped to their canonical
/// template. Keys must be lowercase.
const KIND_ALIASES: &[(&str, &str)] = &[
    ("crd", "customresourcedefinition"),
    ("ds", "daemonset"),
    ("hpa", "horizontalpodautoscaler"),
    ("netpol", "networkpolicy"),
    ("pdb", "poddisruptionbudget"),
    ("persistentvolume", "pv"),
    ("persistentvolumeclaim", "pvc"),
    ("sa", "serviceaccount"),
    ("sts", "statefulset"),
];

/// The kinds `prune` asks the cluster about.
///
/// Only kinds riveter can render: a resource it never created is not its to
/// remove. `raw` is excluded because its `kind` lives in the overlay, and
/// `namespace` because deleting one takes everything inside it with it — far
/// too much to do on the strength of a label.
#[must_use]
pub fn prunable_kinds() -> Vec<&'static str> {
    TEMPLATES
        .iter()
        .map(|(name, _)| name.trim_end_matches(".yaml.j2"))
        .filter(|kind| !matches!(*kind, "_macros" | "raw" | "namespace"))
        .collect()
}

/// Whether an overlay `kind` and a `kubectl -o name` kind are the same thing.
///
/// kubectl prints the plural (`deployments`, `ingresses`), so comparison goes
/// through the canonical spelling with a trailing plural tolerated.
#[must_use]
pub fn kinds_match(overlay_kind: &str, live_kind: &str) -> bool {
    let overlay = canonical_kind(&overlay_kind.to_lowercase());
    let live = canonical_kind(&live_kind.to_lowercase());

    if overlay == live {
        return true;
    }

    // Every plural kubectl might print: `deployments` -> `deployment`,
    // `ingresses` -> `ingress`, `networkpolicies` -> `networkpolicy`. Failing to
    // singularise would make `prune` read a declared resource as orphaned, so
    // all three forms are tried and any match counts.
    [
        live.strip_suffix("ies").map(|stem| format!("{stem}y")),
        live.strip_suffix("es").map(ToString::to_string),
        live.strip_suffix('s').map(ToString::to_string),
    ]
    .into_iter()
    .flatten()
    .any(|singular| canonical_kind(&singular) == overlay)
}

/// Resolves a lowercase `kind` through [`KIND_ALIASES`], leaving unknown kinds
/// untouched.
fn canonical_kind(kind: &str) -> String {
    KIND_ALIASES
        .iter()
        .find(|(alias, _)| *alias == kind)
        .map_or_else(|| kind.to_string(), |(_, target)| (*target).to_string())
}

#[doc(hidden)]
#[must_use]
pub fn template_name_for_kind(kind: &str) -> String {
    format!("{}.yaml.j2", canonical_kind(&kind.to_lowercase()))
}

fn load_embedded_templates(env: &mut Environment<'_>) -> anyhow::Result<()> {
    for (name, source) in TEMPLATES {
        env.add_template(name, source)
            .with_context(|| format!("failed to load embedded template: {name}"))?;
    }
    Ok(())
}

/// Parses every embedded template, so a syntax error is caught by the test
/// suite rather than by the first overlay that happens to use the kind.
#[doc(hidden)]
pub fn check_embedded_templates() -> anyhow::Result<Vec<&'static str>> {
    let mut env = Environment::new();
    env.add_filter("to_yaml", to_yaml);
    load_embedded_templates(&mut env)?;

    Ok(TEMPLATES.iter().map(|(name, _)| *name).collect())
}

/// Renders a value as YAML at column 0, for overlay fields that are passed
/// straight through to Kubernetes (tolerations, affinity, webhooks, ...).
/// Nest the result with minijinja's `indent` filter.
fn to_yaml(value: &Value) -> Result<String, minijinja::Error> {
    serde_yaml::to_string(value)
        .map(|s| s.trim_end().to_string())
        .map_err(|e| {
            minijinja::Error::new(
                minijinja::ErrorKind::InvalidOperation,
                format!("failed to serialize value as yaml: {e}"),
            )
        })
}

/// `${VAR}` references, with `$${VAR}` as the escape for a literal that a later
/// stage is meant to expand instead — a shell in a container `command`, say.
const VAR_PATTERN: &str = r"\$(\$)?\{([^}]+)}";

/// Expands `${VAR}` from the overlay's `.env`.
///
/// An undefined reference is an error rather than a passthrough: left alone it
/// reaches the cluster as the literal string `${VAR}`, which for a Secret means
/// shipping the placeholder as the value. `source` names the file that was
/// consulted, for the error message.
#[doc(hidden)]
pub fn substitute_vars<S: std::hash::BuildHasher>(
    data: &mut YamlValue,
    env: &HashMap<String, String, S>,
    source: Option<&str>,
) -> anyhow::Result<()> {
    let re = Regex::new(VAR_PATTERN)?;
    let mut missing = BTreeMap::new();
    substitute(data, env, &re, "", &mut missing);

    anyhow::ensure!(
        missing.is_empty(),
        "overlay references undefined variable(s):\n{}\n\ndefine them in {}, or write \
         `$${{NAME}}` to keep a literal `${{NAME}}` in the manifest",
        missing
            .iter()
            .map(|(name, path)| format!("  ${{{name}}} at {path}"))
            .collect::<Vec<_>>()
            .join("\n"),
        source.unwrap_or("the overlay's .env file"),
    );

    Ok(())
}

/// Walks the overlay, expanding values and recording every undefined name
/// against the first path it was seen at.
fn substitute<S: std::hash::BuildHasher>(
    value: &mut YamlValue,
    env: &HashMap<String, String, S>,
    re: &Regex,
    path: &str,
    missing: &mut BTreeMap<String, String>,
) {
    match value {
        YamlValue::String(s) => {
            *s = re
                .replace_all(s, |c: &regex::Captures<'_>| {
                    let name = &c[2];
                    if c.get(1).is_some() {
                        return format!("${{{name}}}");
                    }

                    env.get(name).cloned().unwrap_or_else(|| {
                        missing
                            .entry(name.to_string())
                            .or_insert_with(|| path.to_string());
                        c[0].to_string()
                    })
                })
                .into_owned();
        }
        YamlValue::Mapping(m) => {
            for (k, v) in m {
                let key = k.as_str().unwrap_or("?");
                let child = if path.is_empty() {
                    key.to_string()
                } else {
                    format!("{path}.{key}")
                };
                substitute(v, env, re, &child, missing);
            }
        }
        YamlValue::Sequence(s) => {
            for (i, v) in s.iter_mut().enumerate() {
                substitute(v, env, re, &format!("{path}[{i}]"), missing);
            }
        }
        _ => {}
    }
}

/// Drops the blank lines the templates leave behind, without touching the
/// inside of a block scalar.
///
/// A blank line between two manifest keys is template whitespace; the same line
/// inside a `configmap` value or a multi-line `secret` entry is content, and
/// removing it silently rewrites the file the pod will read.
#[must_use]
pub fn strip_empty_lines(s: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    // The block scalar we are inside: its indentation, and whether it keeps the
    // blank lines that trail it (`|+`/`>+`).
    let mut block: Option<BlockScalar> = None;
    // Blank lines held back until we know whether the scalar continues past
    // them: inside it they are content, after it they are template whitespace.
    let mut pending: Vec<&str> = Vec::new();

    for line in s.lines() {
        let blank = line.trim().is_empty();

        if let Some(scalar) = block {
            if blank {
                pending.push(line);
                continue;
            }
            if indent_of(line) > scalar.indent {
                out.append(&mut pending);
                out.push(line);
                continue;
            }
            // Less indented: the scalar ended. Its trailing blank run is part
            // of the value only under keep-chomping; otherwise YAML discards it
            // anyway and it is template whitespace.
            if scalar.keeps_trailing_blanks {
                out.append(&mut pending);
            } else {
                pending.clear();
            }
            block = None;
        }

        if blank {
            continue;
        }
        block = block_scalar(line);
        out.push(line);
    }

    // A kept scalar running to the end of the document keeps its blanks too.
    if block.is_some_and(|s| s.keeps_trailing_blanks) {
        out.append(&mut pending);
    }

    out.join("\n") + "\n"
}

/// An open block scalar being tracked by [`strip_empty_lines`].
#[derive(Debug, Clone, Copy)]
struct BlockScalar {
    indent: usize,
    /// `|+` and `>+` keep the newlines that follow the content; `|`, `|-`, `>`
    /// and `>-` clip or strip them.
    keeps_trailing_blanks: bool,
}

fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// Reads `line` as a block scalar header (`key: |`, `- >-`, `key: |2+`, ...),
/// so the lines belonging to that scalar can be recognised by being indented
/// further.
fn block_scalar(line: &str) -> Option<BlockScalar> {
    let trimmed = line.trim_end();

    // A block indicator is only one if it starts the value, so look at the
    // value rather than at any `|` that happens to sit in the line.
    let value = if let Some((_, v)) = trimmed.rsplit_once(": ") {
        v
    } else {
        trimmed.trim_start().strip_prefix("- ")?
    };

    let mut chars = value.trim_start().chars();
    if !matches!(chars.next(), Some('|' | '>')) {
        return None;
    }

    // An explicit indentation indicator, then an optional chomping indicator.
    let rest = chars
        .as_str()
        .trim_start_matches(|c: char| c.is_ascii_digit());

    matches!(rest, "" | "-" | "+").then(|| BlockScalar {
        indent: indent_of(line),
        keeps_trailing_blanks: rest == "+",
    })
}
