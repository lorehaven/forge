//! `rivet.toml`.

use crate::error::PackageError;
use serde::{Deserialize, Serialize};

/// What a package says about itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    /// Identity and version.
    pub package: PackageMeta,
    /// Constraints checked at install time.
    #[serde(default, skip_serializing_if = "Requires::is_empty")]
    pub requires: Requires,
    /// Free-form, surfaced in UIs and never interpreted here.
    #[serde(default, skip_serializing_if = "toml::Table::is_empty")]
    pub meta: toml::Table,
}

/// The `[package]` table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageMeta {
    /// DNS-1123 label; the registry key.
    pub name: String,
    /// Semver, build metadata allowed (`0.4.0+b123`); immutable once published.
    pub version: String,
    /// One line for catalogs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Default namespace the overlay deploys into.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
}

/// The `[requires]` table.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Requires {
    /// A semver requirement on the installing riveter, e.g. `>=0.3`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub riveter: Option<String>,
    /// Other packages that must be installed, as `name` or `name <requirement>`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub packages: Vec<String>,
}

impl Requires {
    /// Whether nothing is required, so the table can be left out.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.riveter.is_none() && self.packages.is_empty()
    }
}

/// Longest description accepted.
const MAX_DESCRIPTION_LEN: usize = 500;

/// A Kubernetes-style DNS-1123 label: lowercase alphanumerics and `-`, starting
/// and ending alphanumeric, at most 63 characters.
#[must_use]
pub fn is_valid_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    let edge = |b: u8| b.is_ascii_lowercase() || b.is_ascii_digit();
    !bytes.is_empty()
        && bytes.len() <= 63
        && bytes.first().copied().is_some_and(edge)
        && bytes.last().copied().is_some_and(edge)
        && bytes.iter().all(|&b| edge(b) || b == b'-')
}

impl Manifest {
    /// A manifest with just the required fields.
    #[must_use]
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            package: PackageMeta {
                name: name.into(),
                version: version.into(),
                description: None,
                namespace: None,
            },
            requires: Requires::default(),
            meta: toml::Table::new(),
        }
    }

    /// Parses and validates a `rivet.toml`.
    pub fn parse(source: &str) -> Result<Self, PackageError> {
        let manifest: Self =
            toml::from_str(source).map_err(|e| PackageError::Manifest(e.to_string()))?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Renders back to TOML (after validating, so nothing invalid is written).
    pub fn to_toml(&self) -> Result<String, PackageError> {
        self.validate()?;
        toml::to_string(self).map_err(|e| PackageError::Manifest(e.to_string()))
    }

    /// The parsed version.
    pub fn version(&self) -> Result<semver::Version, PackageError> {
        semver::Version::parse(&self.package.version).map_err(|e| {
            PackageError::Manifest(format!(
                "version `{}` is not semver: {e}",
                self.package.version
            ))
        })
    }

    /// Every rule a manifest must keep, whether it was parsed or built in code.
    pub fn validate(&self) -> Result<(), PackageError> {
        let fail = |message: String| Err(PackageError::Manifest(message));

        if !is_valid_name(&self.package.name) {
            return fail(format!(
                "name `{}` must be a lowercase DNS-1123 label (a-z, 0-9, `-`; at most 63 characters)",
                self.package.name
            ));
        }
        self.version()?;

        if let Some(namespace) = &self.package.namespace
            && !is_valid_name(namespace)
        {
            return fail(format!("namespace `{namespace}` must be a DNS-1123 label"));
        }
        if let Some(description) = &self.package.description
            && (description.len() > MAX_DESCRIPTION_LEN || description.contains('\n'))
        {
            return fail(format!(
                "description must be one line of at most {MAX_DESCRIPTION_LEN} characters"
            ));
        }
        if let Some(requirement) = &self.requires.riveter {
            semver::VersionReq::parse(requirement).map_err(|e| {
                PackageError::Manifest(format!("requires.riveter `{requirement}`: {e}"))
            })?;
        }
        for entry in &self.requires.packages {
            parse_package_requirement(entry)?;
        }
        Ok(())
    }
}

/// Splits `name` or `name <requirement>` into its parts.
pub fn parse_package_requirement(
    entry: &str,
) -> Result<(String, semver::VersionReq), PackageError> {
    let entry = entry.trim();
    let (name, requirement) = entry
        .split_once(char::is_whitespace)
        .map_or((entry, ""), |(n, r)| (n, r.trim()));

    if !is_valid_name(name) {
        return Err(PackageError::Manifest(format!(
            "requires.packages entry `{entry}`: `{name}` is not a valid package name"
        )));
    }
    let requirement = if requirement.is_empty() {
        semver::VersionReq::STAR
    } else {
        semver::VersionReq::parse(requirement).map_err(|e| {
            PackageError::Manifest(format!("requires.packages entry `{entry}`: {e}"))
        })?
    };
    Ok((name.to_string(), requirement))
}
