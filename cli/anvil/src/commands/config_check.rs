//! `anvil config check`: validates `.anvil.toml` against anvil's own schema
//! (see `crate::config::Config` and its nested structs).
//!
//! Reports every problem found in one pass, rather than the
//! one-typo-at-a-time story a plain `toml::from_str::<Config>()` gives -
//! unknown fields are silently dropped by serde's default
//! (non-`deny_unknown_fields`) deserialization, so a malformed
//! `[docker.modules.*]` block otherwise only surfaces later, when the
//! affected subcommand runs and something is quietly missing.
//!
//! Works on a raw [`toml::Value`], not `Config` itself: serde's typed
//! deserialization stops at the first error, which is exactly what this
//! command exists to not do.

use anyhow::{Context, Result};
use quench_cli::prelude::{Tone, print_status};
use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use toml::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigIssue {
    /// Dotted path to the offending value, e.g. `docker.modules.docker.dockerfile`.
    pub path: String,
    pub message: String,
}

impl fmt::Display for ConfigIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.path, self.message)
    }
}

const TOP_FIELDS: &[&str] = &["docker", "install", "release"];
const DOCKER_FIELDS: &[&str] = &[
    "registry",
    "cargo_registry",
    "cargo_registry_index",
    "modules",
];
const MODULE_FIELDS: &[&str] = &["packages", "dockerfile"];
const OVERRIDE_FIELDS: &[&str] = &[
    "dockerfile",
    "module_name",
    "image_name",
    "registries",
    "registry",
    "build_args",
];
const INSTALL_FIELDS: &[&str] = &["packages"];
const RELEASE_FIELDS: &[&str] = &["registry", "packages", "commit_message_template"];

fn join(path: &str, key: &str) -> String {
    if path.is_empty() {
        key.to_string()
    } else {
        format!("{path}.{key}")
    }
}

/// Every key in `table` that isn't in `known` - the "malformed block" case
/// this command exists for: serde silently drops these instead of erroring.
fn unknown_fields(
    table: &toml::value::Table,
    known: &[&str],
    path: &str,
    issues: &mut Vec<ConfigIssue>,
) {
    for key in table.keys() {
        if !known.contains(&key.as_str()) {
            issues.push(ConfigIssue {
                path: join(path, key),
                message: format!("unknown field `{key}`"),
            });
        }
    }
}

/// A required field, missing entirely - `#[serde(default)]` on every one of
/// `Config`'s structs makes every field structurally optional to serde, so
/// this command enforces "required" itself rather than through the type
/// system.
// The `map_or_else` rewrite clippy suggests for this match needs two
// closures each capturing `issues` mutably at once, which the borrow checker
// rejects - this shape is the one that actually compiles.
#[allow(clippy::single_match_else, clippy::option_if_let_else)]
fn require<'a>(
    table: &'a toml::value::Table,
    field: &str,
    path: &str,
    issues: &mut Vec<ConfigIssue>,
) -> Option<&'a Value> {
    match table.get(field) {
        Some(v) => Some(v),
        None => {
            issues.push(ConfigIssue {
                path: path.to_string(),
                message: format!("missing required field `{field}`"),
            });
            None
        }
    }
}

fn expect_table<'a>(
    value: &'a Value,
    path: &str,
    issues: &mut Vec<ConfigIssue>,
) -> Option<&'a toml::value::Table> {
    value.as_table().or_else(|| {
        issues.push(ConfigIssue {
            path: path.to_string(),
            message: format!("expected a table, found {}", value.type_str()),
        });
        None
    })
}

fn expect_string(value: &Value, path: &str, issues: &mut Vec<ConfigIssue>) {
    if let Some(s) = value.as_str() {
        if s.is_empty() {
            issues.push(ConfigIssue {
                path: path.to_string(),
                message: "must not be empty".to_string(),
            });
        }
    } else {
        issues.push(ConfigIssue {
            path: path.to_string(),
            message: format!("expected a string, found {}", value.type_str()),
        });
    }
}

/// Returns the array's own string entries; entries of the wrong type are
/// reported and skipped rather than aborting the whole check.
fn expect_string_array(value: &Value, path: &str, issues: &mut Vec<ConfigIssue>) -> Vec<String> {
    let Some(array) = value.as_array() else {
        issues.push(ConfigIssue {
            path: path.to_string(),
            message: format!("expected an array of strings, found {}", value.type_str()),
        });
        return Vec::new();
    };

    array
        .iter()
        .enumerate()
        .filter_map(|(i, entry)| {
            entry.as_str().map(str::to_string).or_else(|| {
                issues.push(ConfigIssue {
                    path: format!("{path}[{i}]"),
                    message: format!("expected a string, found {}", entry.type_str()),
                });
                None
            })
        })
        .collect()
}

/// Parses `content` and validates it, collecting every issue found.
///
/// Rather than stopping at the first, `Err` only for TOML that doesn't parse
/// at all - nothing else can be checked without a value tree to walk.
pub fn validate(content: &str) -> Result<Vec<ConfigIssue>, toml::de::Error> {
    let root: Value = toml::from_str(content)?;
    let mut issues = Vec::new();

    let Some(table) = expect_table(&root, "", &mut issues) else {
        return Ok(issues);
    };
    unknown_fields(table, TOP_FIELDS, "", &mut issues);

    if let Some(docker) = table.get("docker") {
        check_docker(docker, &mut issues);
    }
    if let Some(install) = table.get("install")
        && let Some(t) = expect_table(install, "install", &mut issues)
    {
        unknown_fields(t, INSTALL_FIELDS, "install", &mut issues);
        if let Some(packages) = t.get("packages") {
            expect_string_array(packages, "install.packages", &mut issues);
        }
    }
    if let Some(release) = table.get("release")
        && let Some(t) = expect_table(release, "release", &mut issues)
    {
        unknown_fields(t, RELEASE_FIELDS, "release", &mut issues);
        if let Some(registry) = t.get("registry") {
            expect_string(registry, "release.registry", &mut issues);
        }
        if let Some(packages) = t.get("packages") {
            expect_string_array(packages, "release.packages", &mut issues);
        }
        if let Some(template) = t.get("commit_message_template") {
            expect_string(template, "release.commit_message_template", &mut issues);
        }
    }

    issues.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(issues)
}

fn check_docker(docker: &Value, issues: &mut Vec<ConfigIssue>) {
    let Some(table) = expect_table(docker, "docker", issues) else {
        return;
    };
    unknown_fields(table, DOCKER_FIELDS, "docker", issues);

    if let Some(registry) = table.get("registry") {
        expect_string(registry, "docker.registry", issues);
    }
    if let Some(v) = table.get("cargo_registry") {
        expect_string(v, "docker.cargo_registry", issues);
    }
    if let Some(v) = table.get("cargo_registry_index") {
        expect_string(v, "docker.cargo_registry_index", issues);
    }

    let Some(modules) = table.get("modules") else {
        return;
    };
    let Some(modules_table) = expect_table(modules, "docker.modules", issues) else {
        return;
    };

    // Tracks which module first claimed a package, so a second module
    // claiming the same one is flagged - each package's image build should
    // come from exactly one module.
    let mut package_owner: BTreeMap<String, String> = BTreeMap::new();

    for (module_name, module_value) in modules_table {
        check_module(module_name, module_value, &mut package_owner, issues);
    }
}

fn check_module(
    module_name: &str,
    module_value: &Value,
    package_owner: &mut BTreeMap<String, String>,
    issues: &mut Vec<ConfigIssue>,
) {
    let module_path = format!("docker.modules.{module_name}");
    let Some(module_table) = expect_table(module_value, &module_path, issues) else {
        return;
    };

    let packages = require(module_table, "packages", &module_path, issues)
        .map(|v| expect_string_array(v, &format!("{module_path}.packages"), issues))
        .unwrap_or_default();
    if module_table.contains_key("packages") && packages.is_empty() {
        issues.push(ConfigIssue {
            path: format!("{module_path}.packages"),
            message: "must not be empty".to_string(),
        });
    }

    if let Some(v) = require(module_table, "dockerfile", &module_path, issues) {
        expect_string(v, &format!("{module_path}.dockerfile"), issues);
    }

    for package in &packages {
        if let Some(existing) = package_owner.insert(package.clone(), module_name.to_string())
            && existing != module_name
        {
            issues.push(ConfigIssue {
                path: format!("{module_path}.packages"),
                message: format!("package `{package}` is also built by module `{existing}`"),
            });
        }
    }

    // Anything besides `packages`/`dockerfile` is a flattened package
    // override, keyed by package name (see `DockerModuleConfig::package_overrides`).
    for (key, value) in module_table {
        if MODULE_FIELDS.contains(&key.as_str()) {
            continue;
        }
        check_package_override(key, value, &module_path, &packages, issues);
    }
}

fn check_package_override(
    package: &str,
    value: &Value,
    module_path: &str,
    packages: &[String],
    issues: &mut Vec<ConfigIssue>,
) {
    let override_path = format!("{module_path}.{package}");
    let Some(table) = expect_table(value, &override_path, issues) else {
        return;
    };
    unknown_fields(table, OVERRIDE_FIELDS, &override_path, issues);

    if !packages.contains(&package.to_string()) {
        issues.push(ConfigIssue {
            path: override_path.clone(),
            message: format!(
                "overrides package `{package}`, which is not in this module's `packages` list"
            ),
        });
    }

    for field in ["dockerfile", "module_name", "image_name", "registry"] {
        if let Some(v) = table.get(field) {
            expect_string(v, &format!("{override_path}.{field}"), issues);
        }
    }
    if let Some(v) = table.get("registries") {
        expect_string_array(v, &format!("{override_path}.registries"), issues);
    }
    if table.contains_key("registry") && table.contains_key("registries") {
        issues.push(ConfigIssue {
            path: override_path.clone(),
            message: "sets both the deprecated `registry` and `registries` - use `registries` only"
                .to_string(),
        });
    }

    if let Some(build_args) = table.get("build_args")
        && let Some(build_args_table) =
            expect_table(build_args, &format!("{override_path}.build_args"), issues)
    {
        for (arg_name, arg_value) in build_args_table {
            expect_string(
                arg_value,
                &format!("{override_path}.build_args.{arg_name}"),
                issues,
            );
        }
    }
}

/// `anvil config check`'s entry point.
///
/// Reads `.anvil.toml` from the current directory (deliberately not
/// `config::load_config`, which tolerates a missing/invalid file by
/// defaulting - the whole point here is to fail loudly instead).
pub fn check() -> Result<()> {
    let content = fs::read_to_string(".anvil.toml").context("Failed to read .anvil.toml")?;

    let issues = match validate(&content) {
        Ok(issues) => issues,
        Err(err) => {
            print_status(
                Tone::Error,
                "anvil",
                &format!(".anvil.toml is not valid TOML: {err}"),
            );
            anyhow::bail!("config check failed");
        }
    };

    if issues.is_empty() {
        print_status(Tone::Success, "anvil", ".anvil.toml is valid");
        return Ok(());
    }

    print_status(
        Tone::Error,
        "anvil",
        &format!(
            "{} error{} in .anvil.toml:",
            issues.len(),
            if issues.len() == 1 { "" } else { "s" }
        ),
    );
    for issue in &issues {
        println!("  {issue}");
    }
    anyhow::bail!("{} error(s) in .anvil.toml", issues.len());
}
