use crate::env::{OUTPUT_DIR, OVERLAY_DIR, manifest_path};
use anyhow::Context;
use minijinja::{Environment, Value, context};
use regex::Regex;
use serde_yaml::Value as YamlValue;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceScope {
    Mutable,
    Immutable,
    All,
}

#[derive(Debug, Clone)]
pub struct RenderedManifest {
    pub path: String,
    pub resource_count: usize,
}

pub fn generate_manifests(env_name: &str) -> anyhow::Result<String> {
    Ok(generate_manifests_with_scope(env_name, ResourceScope::All)?.path)
}

pub fn generate_manifests_with_scope(
    env_name: &str,
    scope: ResourceScope,
) -> anyhow::Result<RenderedManifest> {
    let env_vars = load_env(env_name)?;

    let data = render_overlay(env_name, &env_vars)?;
    let rendered_resources = render_resources(env_name, &data, scope)?;

    fs::create_dir_all(OUTPUT_DIR)?;
    let path = scoped_manifest_path(env_name, scope);
    fs::write(
        &path,
        if rendered_resources.is_empty() {
            String::new()
        } else {
            strip_empty_lines(&(rendered_resources.join("\n---\n") + "\n"))
        },
    )?;

    Ok(RenderedManifest {
        path,
        resource_count: rendered_resources.len(),
    })
}

fn render_overlay(env_name: &str, env_vars: &HashMap<String, String>) -> anyhow::Result<YamlValue> {
    let overlay_src = fs::read_to_string(format!("{OVERLAY_DIR}/{env_name}/overlay.yaml"))?;

    let mut overlay_jinja = Environment::new();
    overlay_jinja.set_loader(minijinja::path_loader(OVERLAY_DIR));
    overlay_jinja.add_global("env", env_name);

    let rendered_overlay = overlay_jinja.render_str(&overlay_src, Value::UNDEFINED)?;
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

    let re = Regex::new(r"\$\{([^}]+)}")?;
    substitute(&mut data, env_vars, &re);

    Ok(data)
}

fn render_resources(
    env_name: &str,
    data: &YamlValue,
    scope: ResourceScope,
) -> anyhow::Result<Vec<String>> {
    let resources = data["resources"]
        .as_sequence()
        .context("resources must be a list")?;

    let mut tpl_env = Environment::new();
    load_embedded_templates(&mut tpl_env)?;
    tpl_env.add_global("env", env_name);
    tpl_env.add_filter("to_yaml", to_yaml);

    let mut out = Vec::new();
    for res in resources {
        if !resource_in_scope(res, scope) {
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
        out.push(y.trim().to_string());
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

fn load_env(env: &str) -> anyhow::Result<HashMap<String, String>> {
    let env_path = format!("{OVERLAY_DIR}/{env}/.env");
    let path = if Path::new(&env_path).exists() {
        env_path
    } else if Path::new(".env").exists() {
        ".env".to_string()
    } else {
        return Ok(HashMap::new());
    };

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

    Ok(env_vars)
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

#[doc(hidden)]
#[must_use]
pub fn template_name_for_kind(kind: &str) -> String {
    let kind = kind.to_lowercase();
    let canonical = KIND_ALIASES
        .iter()
        .find(|(alias, _)| *alias == kind)
        .map_or(kind.as_str(), |(_, target)| *target);

    format!("{canonical}.yaml.j2")
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

fn substitute(value: &mut YamlValue, env: &HashMap<String, String>, re: &Regex) {
    match value {
        YamlValue::String(s) => {
            *s = re
                .replace_all(s, |c: &regex::Captures<'_>| {
                    env.get(&c[1]).cloned().unwrap_or_else(|| c[0].to_string())
                })
                .into_owned();
        }
        YamlValue::Mapping(m) => m.values_mut().for_each(|v| substitute(v, env, re)),
        YamlValue::Sequence(s) => s.iter_mut().for_each(|v| substitute(v, env, re)),
        _ => {}
    }
}

#[must_use]
pub fn strip_empty_lines(s: &str) -> String {
    s.lines()
        .filter(|l| !l.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}
