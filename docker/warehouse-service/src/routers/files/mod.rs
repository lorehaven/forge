//! Plain file storage, addressed by path within a named storage.
//!
//! Neither registry fits everything the estate produces. A build output is not
//! a crate and not an image: it has a name somebody chose, it is fetched back
//! whole, and nothing resolves it by version. Conveyor's artifacts are the
//! first caller, and the shape is deliberately dull - PUT a path, GET it back.
//!
//! ## Storages
//!
//! A *storage* is a name bound to a directory, configured by the operator:
//!
//! ```text
//! FILE_STORAGES=artifacts=/storage/artifacts;media=/mnt/media
//! ```
//!
//! Callers name the storage and never the directory, so the host's layout is
//! not something an API client can learn or depend on. A name that is not
//! configured is a 404 - there is no implicit creation, because a typo would
//! otherwise silently start a new pile of files nobody is watching.
//!
//! ## Paths
//!
//! The `path` query parameter is the only caller-controlled part of where a
//! file lands, and it is the whole attack surface of this module. See
//! [`resolve`]: `..` is refused outright rather than normalised away, absolute
//! paths are refused, and the result is checked against the storage root again
//! after the filesystem has had its say, so a symlink planted inside a storage
//! cannot be used to read or write outside it.

use actix_web::dev::HttpServiceFactory;
use actix_web::middleware::NormalizePath;
use actix_web::web;
use quench_auth::actix::middleware::auth::Auth;
use quench_auth::prelude::JwtConfig;
use serde::Deserialize;
use std::path::{Component, Path, PathBuf};

pub mod authz;
pub mod dynamic;
pub mod ops;
pub mod pagination;

/// A name bound to a directory on disk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Storage {
    pub name: String,
    pub root: PathBuf,
}

/// The configured storages, parsed once.
static STORAGES: std::sync::LazyLock<Vec<Storage>> =
    std::sync::LazyLock::new(|| parse_storages(&envmnt::get_or("FILE_STORAGES", "")));

/// The most a single file may be, streamed or not.
///
/// Matched to `MAX_REQUEST_BODY_BYTES` by default because the two limits mean
/// the same thing here: the body *is* the file.
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

/// Whether `name` is safe to use as a storage name - static or dynamic alike.
/// Names appear in a URL path segment, so keeping them to this set means one
/// can never be something that has to be escaped, or something that looks
/// like a path of its own.
pub fn valid_storage_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// `name=path` pairs separated by `;`.
///
/// A malformed entry is dropped with a warning rather than taken as a fatal
/// error: losing one storage is better than refusing to start and taking the
/// crates and docker registries down with it.
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

        // The name appears in a URL path segment. Keeping it to this set means
        // a storage can never be named something that has to be escaped, or
        // something that looks like a path of its own.
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

// ---------------------------------------------------------------------------
// Path handling
// ---------------------------------------------------------------------------

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
///
/// `..` is **rejected**, not resolved. Normalising it away is the usual
/// approach and it is the one that keeps going wrong: `a/../../b` collapses
/// correctly only if you also account for what `a` is, and if `a` is a symlink
/// the lexical answer and the filesystem's answer differ. Refusing the
/// component outright means there is no arithmetic to get wrong, and no
/// legitimate caller needs it - the whole path is being chosen by whoever is
/// uploading.
pub fn relative(path: &str) -> Result<PathBuf, PathError> {
    if path.trim().is_empty() {
        return Err(PathError::Empty);
    }

    // A NUL truncates the path at the syscall boundary, so a name that passed
    // every check above it is not the name that gets opened. Control bytes are
    // refused with it: they have no legitimate use and they make a path
    // unreadable in a log.
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

/// Whether `target` is really inside `root` once the filesystem has resolved
/// every symlink on the way.
///
/// [`relative`] already guarantees the path *spells* something inside the
/// storage. This answers the different question of whether it *is*: a symlink
/// sitting in the storage - planted by an earlier upload, or by whatever else
/// has write access to that directory - points wherever it likes, and following
/// it would read or overwrite a file outside.
///
/// The target of a write does not exist yet, so the deepest ancestor that does
/// exist is the one checked; the file is created inside it either way.
pub async fn confined(root: &Path, target: &Path) -> bool {
    let Ok(root) = tokio::fs::canonicalize(root).await else {
        // A storage whose directory does not exist confines nothing. Callers
        // turn this into a 404 rather than creating it: a storage root is the
        // operator's to provide.
        return false;
    };

    let mut probe = target;
    loop {
        match tokio::fs::canonicalize(probe).await {
            Ok(real) => return real.starts_with(&root),
            Err(_) => match probe.parent() {
                Some(parent) => probe = parent,
                // Walked past the filesystem root without finding anything
                // that exists, which cannot happen for a path built from a
                // storage root - but it is not a reason to allow the write.
                None => return false,
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Query
// ---------------------------------------------------------------------------

/// `?path=` - which file within the storage.
#[derive(Debug, Deserialize)]
pub struct FileQuery {
    #[serde(default)]
    pub path: String,
}

/// `?prefix=&n=&last=&desc=` - which subtree to list, how much of it at once,
/// and in which direction. `n`/`last` mirror `routers::docker::registry::
/// catalog`'s own pagination rather than a second convention in the same
/// service: `n` is the page size, `last` the final item's key from the
/// previous page (exclusive). `desc` walks newest-first instead of
/// oldest-first - a backup client's browse view wants the files it just
/// uploaded at the top, not buried behind everything older.
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

// ---------------------------------------------------------------------------
// Actix scope
// ---------------------------------------------------------------------------

/// Route order is load-bearing: the concrete `/{storage}/file` and
/// `/{storage}/sync` have to be registered before `/{storage}`, which would
/// otherwise match them and treat `file`/`sync` as a storage name.
///
/// Unlike before dynamic storages existed, there is no blanket `RequireWrite`
/// here: a static storage's upload/delete still check the blanket
/// `warehouse:write` grant themselves (see `ops::mod::storage_or_error`), but
/// a dynamic storage's access depends on who owns it and what's been shared
/// with whom, which only `authz::can_on_storage` can answer - see that
/// module's docs for why this diverges from `RequireWrite`'s usual role.
/// `Auth` stays mounted so every handler has claims to check.
pub fn scope(jwt_config: JwtConfig) -> impl HttpServiceFactory {
    web::scope("/api/v1/files")
        .wrap(NormalizePath::trim())
        .wrap(Auth::new(jwt_config))
        .service(ops::storages::create)
        .service(ops::storages::patch)
        .service(ops::storages::remove)
        .service(ops::storages::sync_log)
        .service(ops::upload::handle)
        .service(ops::download::handle)
        .service(ops::download::head)
        .service(ops::delete::handle)
        .service(ops::list::storages)
        .service(ops::list::entries)
}
