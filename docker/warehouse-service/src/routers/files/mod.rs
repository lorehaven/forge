//! Plain file storage, addressed by path within a named, operator-configured
//! storage (`FILE_STORAGES=`) - `path` is the only caller-controlled part; see [`resolve`].

use async_trait::async_trait;
use quench_auth::domain::jwt::{Claims, JwtConfig};
use quench_auth::http::middleware::auth::Auth;
use quench_http::prelude::{Endpoint, FromRequest, HttpError, OnPathPrefix, Request, wrap};
use serde::Deserialize;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

pub mod authz;
pub mod dynamic;
pub mod ops;
pub mod pagination;

/// The verified identity behind this request, if any - read from `Auth`'s extensions.
pub struct OptionalClaims(pub Option<Claims>);

#[async_trait]
impl FromRequest for OptionalClaims {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        Ok(Self(req.extensions().get::<Claims>().cloned()))
    }
}

/// A name bound to a directory on disk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Storage {
    pub name: String,
    pub root: PathBuf,
}

/// The configured storages, parsed once.
static STORAGES: std::sync::LazyLock<Vec<Storage>> =
    std::sync::LazyLock::new(|| parse_storages(&envmnt::get_or("FILE_STORAGES", "")));

/// The most a single file may be. Defaults to `MAX_REQUEST_BODY_BYTES` - here the body *is* the file.
static MAX_FILE_BYTES: std::sync::LazyLock<u64> = std::sync::LazyLock::new(|| {
    let loader = quench_config::ConfigLoader::new("WAREHOUSE");
    loader.env_u64(
        "MAX_FILE_BYTES",
        loader.env_u64("MAX_REQUEST_BODY_BYTES", 1024 * 1024 * 1024),
    )
});

pub fn max_file_bytes() -> u64 {
    *MAX_FILE_BYTES
}

/// Whether `name` is safe as a storage name (URL path segment) - static or dynamic alike.
pub fn valid_storage_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// `name=path` pairs separated by `;`. A malformed entry warns and is dropped,
/// rather than failing startup and taking the other registries down with it.
pub fn parse_storages(raw: &str) -> Vec<Storage> {
    let mut storages: Vec<Storage> = Vec::new();

    for entry in raw.split(';').map(str::trim).filter(|e| !e.is_empty()) {
        let Some((name, root)) = entry.split_once('=') else {
            tracing::warn!("ignoring file storage `{entry}`: expected `name=path`");
            continue;
        };

        let name = name.trim();
        let root = root.trim();

        if name.is_empty() || root.is_empty() {
            tracing::warn!("ignoring file storage `{entry}`: empty name or path");
            continue;
        }

        if !valid_storage_name(name) {
            tracing::warn!(
                "ignoring file storage `{name}`: names may use letters, digits, `-` and `_` only"
            );
            continue;
        }

        if storages.iter().any(|existing| existing.name == name) {
            tracing::warn!("ignoring duplicate file storage `{name}`");
            continue;
        }

        storages.push(Storage {
            name: name.to_string(),
            root: PathBuf::from(root),
        });
    }

    storages
}

/// The configured storages.
pub fn storages() -> &'static [Storage] {
    &STORAGES
}

/// One storage by name, or `None` when this deployment has no such storage.
pub fn storage(name: &str) -> Option<&'static Storage> {
    STORAGES.iter().find(|storage| storage.name == name)
}

/// Says at startup what this deployment will serve, so a missing storage is
/// visible in the log rather than only in a caller's 404.
pub fn report_storages() {
    if !crate::routers::files_enabled() {
        tracing::info!("file storage is disabled (FEATURE_FILES_ENABLED)");
        return;
    }

    if STORAGES.is_empty() {
        tracing::warn!(
            "file storage is enabled but FILE_STORAGES is empty: every request will be a 404"
        );
        return;
    }

    for storage in STORAGES.iter() {
        tracing::info!(
            "file storage `{}` -> {}",
            storage.name,
            storage.root.display()
        );
    }
}

// --- Path handling ---

/// Why a caller's path was refused.
#[derive(Debug, PartialEq, Eq)]
pub enum PathError {
    /// No path, or one that means the storage root itself.
    Empty,
    /// Rooted at `/`, or carrying a drive prefix.
    Absolute,
    /// Contains a `..` component.
    Traversal,
    /// Contains a byte that has no business in a file name.
    Invalid,
}

impl PathError {
    pub fn message(&self) -> &'static str {
        match self {
            Self::Empty => "path is required",
            Self::Absolute => "path must be relative to the storage",
            Self::Traversal => "path must not contain `..`",
            Self::Invalid => "path contains invalid characters",
        }
    }
}

/// A caller's path, as a relative path that cannot leave the storage.
/// `..` is **rejected**, not normalised - a symlink makes lexical resolution wrong anyway.
pub fn relative(path: &str) -> Result<PathBuf, PathError> {
    if path.trim().is_empty() {
        return Err(PathError::Empty);
    }

    // A NUL truncates at the syscall boundary; control bytes ride along too.
    if path.bytes().any(|b| b < 0x20 || b == 0x7f) {
        return Err(PathError::Invalid);
    }

    let mut resolved = PathBuf::new();

    for component in Path::new(path).components() {
        match component {
            Component::Normal(part) => resolved.push(part),
            // `./thing` is just `thing`; harmless and worth accepting.
            Component::CurDir => {}
            Component::ParentDir => return Err(PathError::Traversal),
            Component::RootDir | Component::Prefix(_) => return Err(PathError::Absolute),
        }
    }

    if resolved.as_os_str().is_empty() {
        return Err(PathError::Empty);
    }

    Ok(resolved)
}

/// Where a caller's path lands inside a storage.
pub fn resolve(storage: &Storage, path: &str) -> Result<PathBuf, PathError> {
    Ok(storage.root.join(relative(path)?))
}

/// Whether `target` is really inside `root` after resolving every symlink -
/// [`relative`] only guarantees the path *spells* something inside the storage.
pub async fn confined(root: &Path, target: &Path) -> bool {
    let Ok(root) = tokio::fs::canonicalize(root).await else {
        // A storage whose directory doesn't exist confines nothing.
        return false;
    };

    let mut probe = target;
    loop {
        match tokio::fs::canonicalize(probe).await {
            Ok(real) => return real.starts_with(&root),
            Err(_) => match probe.parent() {
                Some(parent) => probe = parent,
                None => return false,
            },
        }
    }
}

// --- Query ---

/// `?path=` - which file. `?disposition=inline` asks for in-place rendering
/// instead of an attachment download - used by the management UI's preview pane.
#[derive(Debug, Deserialize)]
pub struct FileQuery {
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub disposition: Option<String>,
}

/// `?prefix=&n=&last=&desc=` - subtree, page size, last key (exclusive), and
/// sort direction; `desc` puts newest first, for a backup client's browse view.
#[derive(Debug, Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    pub prefix: Option<String>,
    #[serde(default)]
    pub n: Option<usize>,
    #[serde(default)]
    pub last: Option<String>,
    #[serde(default)]
    pub desc: bool,
}

// --- Auth wrapping ---

/// No blanket `RequireWrite`: static storages check `warehouse:write`
/// themselves, dynamic ones defer to `authz::can_on_storage`; `Auth` stays mounted for claims.
pub fn wrap_auth(
    app: Arc<dyn Endpoint>,
    jwt_config: JwtConfig,
    base_path: &str,
) -> Arc<dyn Endpoint> {
    let prefix: &'static str = Box::leak(format!("{base_path}/api/v1/files").into_boxed_str());
    wrap(app, OnPathPrefix::new(prefix, Auth::new(jwt_config)))
}

pub fn register_routes() {
    ops::register_routes();
}
