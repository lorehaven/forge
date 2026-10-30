//! Finds every route a service declares, straight from its source.
//!
//! Routes are registered by `#[get("/path")]`-style attributes whose path is the full request path
//! (a runtime `BASE_PATH` is applied on top), so reading the attributes gives the real attack
//! surface without running anything. A route added tomorrow is in tomorrow's census automatically.

use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route {
    pub service: String,
    pub method: String,
    pub pattern: String,
    pub source: String,
}

impl Route {
    #[must_use]
    pub fn new(service: &str, method: &str, pattern: &str) -> Self {
        Self {
            service: service.to_string(),
            method: method.to_ascii_uppercase(),
            pattern: pattern.to_string(),
            source: String::new(),
        }
    }

    /// The key a baseline file uses for this route.
    #[must_use]
    pub fn key(&self) -> String {
        format!("{} {} {}", self.service, self.method, self.pattern)
    }

    #[must_use]
    pub fn is_mutating(&self) -> bool {
        matches!(self.method.as_str(), "POST" | "PUT" | "PATCH" | "DELETE")
    }

    #[must_use]
    pub fn has_params(&self) -> bool {
        self.pattern.contains('{')
    }

    #[must_use]
    pub fn pieces(&self) -> Vec<Piece> {
        pieces(&self.pattern)
    }

    /// A concrete path with every parameter replaced by a value that cannot name a real resource.
    #[must_use]
    pub fn sample_path(&self) -> String {
        render(&self.pieces(), None)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Piece {
    Literal(String),
    Param {
        name: String,
        constraint: Option<String>,
    },
}

/// Splits `/v2/{name:.+}/blobs/{digest}` into literals and parameters.
#[must_use]
pub fn pieces(pattern: &str) -> Vec<Piece> {
    let mut out = Vec::new();
    let mut literal = String::new();
    let mut chars = pattern.chars();
    while let Some(c) = chars.next() {
        if c != '{' {
            literal.push(c);
            continue;
        }
        let mut inner = String::new();
        let mut depth = 1;
        for next in chars.by_ref() {
            match next {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
            inner.push(next);
        }
        if !literal.is_empty() {
            out.push(Piece::Literal(std::mem::take(&mut literal)));
        }
        let (name, constraint) = match inner.split_once(':') {
            Some((n, r)) => (n.to_string(), Some(r.to_string())),
            None => (inner, None),
        };
        out.push(Piece::Param { name, constraint });
    }
    if !literal.is_empty() {
        out.push(Piece::Literal(literal));
    }
    out
}

/// A harmless, shape-correct value for a parameter.
#[must_use]
pub fn sample_value(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    if lower.contains("uuid") || lower == "id" || lower.ends_with("_id") || lower.ends_with("id") {
        "00000000-0000-0000-0000-000000000000".to_string()
    } else if lower.contains("version") {
        "0.0.0-sectest".to_string()
    } else if lower.contains("digest") {
        format!("sha256:{}", "0".repeat(64))
    } else {
        "sectest".to_string()
    }
}

/// Renders pieces; `replace` swaps one parameter (by index into the pieces) for a hostile value.
#[must_use]
pub fn render(pieces: &[Piece], replace: Option<(usize, &str)>) -> String {
    let mut out = String::new();
    for (i, piece) in pieces.iter().enumerate() {
        match piece {
            Piece::Literal(text) => out.push_str(text),
            Piece::Param { name, .. } => match replace {
                Some((at, value)) if at == i => out.push_str(value),
                _ => out.push_str(&sample_value(name)),
            },
        }
    }
    if out.is_empty() { "/".to_string() } else { out }
}

const METHODS: [&str; 5] = ["get", "post", "put", "delete", "patch"];

/// Routes declared in one source text.
#[must_use]
pub fn parse_source(service: &str, source: &str, origin: &str) -> Vec<Route> {
    let mut routes = Vec::new();
    for (index, line) in source.lines().enumerate() {
        let line = line.trim_start();
        let Some(rest) = line.strip_prefix("#[") else {
            continue;
        };
        let Some(method) = METHODS.iter().find(|m| {
            rest.strip_prefix(**m)
                .is_some_and(|after| after.starts_with('('))
        }) else {
            continue;
        };
        let after = &rest[method.len() + 1..];
        let Some(start) = after.find('"') else {
            continue;
        };
        let Some(len) = after[start + 1..].find('"') else {
            continue;
        };
        let pattern = &after[start + 1..start + 1 + len];
        if !pattern.starts_with('/') {
            continue;
        }
        let mut route = Route::new(service, method, pattern);
        route.source = format!("{origin}:{}", index + 1);
        routes.push(route);
    }
    routes
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path
                .file_name()
                .is_some_and(|n| n == "target" || n == "tests")
            {
                continue;
            }
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Every route under `source_dir`, attributed to `service`.
#[must_use]
pub fn scan_dir(service: &str, source_dir: &Path) -> Vec<Route> {
    let mut files = Vec::new();
    rust_files(source_dir, &mut files);
    files.sort();
    let mut routes = Vec::new();
    for file in files {
        if let Ok(text) = fs::read_to_string(&file) {
            routes.extend(parse_source(service, &text, &file.display().to_string()));
        }
    }
    routes
}

/// The service directories of a forge checkout: `(name, directory)` for every `services/<name>/service`, in name order.
#[must_use]
pub fn service_dirs(forge_root: &Path) -> Vec<(String, PathBuf)> {
    let Ok(entries) = fs::read_dir(forge_root.join("services")) else {
        return Vec::new();
    };
    let mut found: Vec<(String, PathBuf)> = entries
        .flatten()
        .filter_map(|e| {
            let dir = e.path().join("service");
            let name = e.file_name().to_str()?.to_string();
            dir.join("Cargo.toml").is_file().then_some((name, dir))
        })
        .collect();
    found.sort();
    found
}

/// Routes of every `services/<name>/service` in a forge checkout (plus `extra_dirs`, e.g. the
/// quench-starter sources whose health/auth routes every service links in).
#[must_use]
pub fn scan_forge(forge_root: &Path, extra_dirs: &[PathBuf]) -> Vec<Route> {
    let mut routes = Vec::new();
    for (service, dir) in service_dirs(forge_root) {
        let service = service.as_str();
        let mut found = scan_dir(service, &dir.join("src"));
        for extra in extra_dirs {
            found.extend(scan_dir(service, extra));
        }
        routes.extend(found);
    }
    routes
}

/// Where the shared quench routes (health, auth, swagger - every service links them in) can be read.
///
/// In order: `FORGE_SECTEST_QUENCH_DIR`; a `quench` checkout next to `forge_root`; the exact
/// `quench-starter`/`quench-auth` versions this workspace is locked to, found with `cargo metadata`
/// (which makes sure they are downloaded - in CI a job that compiles only this crate would otherwise
/// have none); and, last, the newest copies already sitting in cargo's registry cache.
#[must_use]
pub fn quench_route_sources(forge_root: &Path) -> Vec<PathBuf> {
    let from = |quench: &Path| {
        vec![
            quench.join("quench-starter/src"),
            quench.join("quench-auth/src"),
        ]
    };
    if let Some(dir) = std::env::var_os("FORGE_SECTEST_QUENCH_DIR") {
        return from(Path::new(&dir));
    }
    let sibling = forge_root.join("../quench");
    if sibling.join("quench-starter/src").is_dir() {
        return from(&sibling);
    }
    let locked = locked_route_sources(forge_root);
    if !locked.is_empty() {
        return locked;
    }
    cached_route_sources()
}

/// The crates' `src` directories at the versions `forge_root`'s lockfile resolves, via `cargo metadata`.
#[must_use]
pub fn locked_route_sources(forge_root: &Path) -> Vec<PathBuf> {
    use quench_serde::Value;

    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let Ok(output) = std::process::Command::new(cargo)
        .args(["metadata", "--format-version", "1", "--manifest-path"])
        .arg(forge_root.join("Cargo.toml"))
        .output()
    else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    let Ok(metadata) = quench_serde::json::from_slice::<Value>(&output.stdout) else {
        return Vec::new();
    };
    let Some(packages) = metadata.get("packages").and_then(Value::as_array) else {
        return Vec::new();
    };

    let mut found = Vec::new();
    for wanted in ["quench-starter", "quench-auth"] {
        let manifest = packages
            .iter()
            .find(|package| package.get("name").and_then(Value::as_str) == Some(wanted))
            .and_then(|package| package.get("manifest_path"))
            .and_then(Value::as_str);
        if let Some(src) = manifest
            .map(Path::new)
            .and_then(Path::parent)
            .map(|dir| dir.join("src"))
            .filter(|src| src.is_dir())
        {
            found.push(src);
        }
    }
    found
}

/// The newest `quench-starter`/`quench-auth` already unpacked in cargo's registry cache, if any.
fn cached_route_sources() -> Vec<PathBuf> {
    let cargo_home = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| Path::new(&home).join(".cargo")));
    let Some(registry) = cargo_home.map(|home| home.join("registry/src")) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for crate_name in ["quench-starter", "quench-auth"] {
        let mut candidates: Vec<PathBuf> = fs::read_dir(&registry)
            .into_iter()
            .flatten()
            .flatten()
            .flat_map(|index| fs::read_dir(index.path()).into_iter().flatten().flatten())
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
                    n.strip_prefix(crate_name).is_some_and(|rest| {
                        rest.starts_with('-') && rest[1..].starts_with(|c: char| c.is_ascii_digit())
                    })
                })
            })
            .collect();
        // "quench-starter-0.4.14" sorts after "...0.4.9" as text; compare the version numerically.
        candidates.sort_by_key(|path| version_key(path));
        if let Some(newest) = candidates.pop() {
            found.push(newest.join("src"));
        }
    }
    found
}

fn version_key(path: &Path) -> Vec<u64> {
    path.file_name()
        .and_then(|n| n.to_str())
        .and_then(|n| n.rsplit('-').next())
        .map(|v| v.split('.').map(|part| part.parse().unwrap_or(0)).collect())
        .unwrap_or_default()
}
