//! Books released to Warehouse by `anvil docs release`, read the way lectern reads them.
//!
//! ```text
//! <book>/latest.json                       the manifest of the newest release, plus where its archive is
//! <book>/<version>/manifest.json
//! <book>/<version>/<book>-<version>.tar.gz the built site, and the Markdown under `_source/`
//! ```
//!
//! loupe takes `latest.json` (or the version's `manifest.json`), downloads the archive, and unpacks only its
//! `_source/` into `$XDG_CACHE_HOME/loupe/<book>/<version>/` (else `~/.cache/loupe/...`). That directory is then
//! an ordinary book. A version already in the cache is not downloaded again.
//!
//! Warehouse is reached the way the other forge tools reach it: its URL from `LOUPE_WAREHOUSE_URL`, loupe's
//! config file, or the `[docs]` section of an `.anvil.toml` (or `.anvil/`) above the working directory; a
//! bearer token from `LOUPE_WAREHOUSE_TOKEN`, the application token `loupe login` stores (gatehouse-auth), or
//! anvil's stored credentials; without any, the request goes without a token, which a storage that allows
//! reading without one answers.

use quench_serde::Value;
use std::path::{Component, Path, PathBuf};

/// Where the books are: Warehouse (with its base path, no trailing slash) and the files storage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Remote {
    pub warehouse: String,
    pub storage: String,
}

/// The storage anvil releases books to unless `.anvil.toml` says otherwise, and lectern's default.
pub const DEFAULT_STORAGE: &str = "docs";

/// Most an archive may unpack to.
const MAX_UNPACKED: usize = 1 << 30;

/// What `anvil docs release` puts the Markdown under.
pub const SOURCES: &str = "_source/";

/// Network and credentials, as loupe needs them; Warehouse in service, a map in tests.
pub trait Net {
    /// `GET url`, with `token` as a bearer token: the status and the body.
    ///
    /// # Errors
    /// Warehouse cannot be reached.
    fn get(&self, url: &str, token: Option<&str>) -> Result<(u16, Vec<u8>), String>;

    /// The bearer token to send: `None` without credentials.
    ///
    /// # Errors
    /// There are credentials, but exchanging them for a token failed.
    fn token(&self) -> Result<Option<String>, String>;
}

/// The `[docs]` (and `[artifacts]`, `[release]`) settings of the nearest anvil configuration above `start`:
/// `.anvil.toml`, then `.anvil/*.toml` by name, later files winning.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AnvilSettings {
    pub warehouse_url: Option<String>,
    pub storage: Option<String>,
    pub gatehouse_url: Option<String>,
}

impl AnvilSettings {
    /// Reads the nearest anvil configuration above `start`; nothing when there is none (or it is not TOML).
    #[must_use]
    pub fn find(start: &Path) -> Self {
        let Some(root) = start
            .ancestors()
            .find(|dir| dir.join(".anvil.toml").is_file() || dir.join(".anvil").is_dir())
        else {
            return Self::default();
        };
        let mut files = vec![root.join(".anvil.toml")];
        if let Ok(entries) = std::fs::read_dir(root.join(".anvil")) {
            let mut more: Vec<PathBuf> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "toml"))
                .filter(|p| p.file_name().is_some_and(|n| n != "secret.toml"))
                .collect();
            more.sort();
            files.extend(more);
        }
        let mut settings = Self::default();
        for file in files {
            let Ok(text) = std::fs::read_to_string(&file) else {
                continue;
            };
            let Ok(value) = quench_serde::toml::from_str::<Value>(&text) else {
                continue;
            };
            let get = |table: &str, key: &str| {
                value
                    .get(table)
                    .and_then(|t| t.get(key))
                    .and_then(Value::as_str)
                    .filter(|v| !v.trim().is_empty())
                    .map(str::to_string)
            };
            if let Some(url) =
                get("docs", "warehouse_url").or_else(|| get("artifacts", "warehouse_url"))
            {
                settings.warehouse_url = Some(url);
            }
            if let Some(storage) = get("docs", "storage") {
                settings.storage = Some(storage);
            }
            if let Some(url) = get("release", "gatehouse_url") {
                settings.gatehouse_url = Some(url);
            }
        }
        settings
    }
}

/// Where the books are: the first Warehouse URL of `LOUPE_WAREHOUSE_URL`, loupe's config, and anvil's; the
/// storage likewise (`LOUPE_STORAGE`, the config's `storage`, anvil's `[docs] storage`, else `docs`).
///
/// # Errors
/// No Warehouse URL anywhere.
pub fn remote(
    env_url: Option<&str>,
    env_storage: Option<&str>,
    config_url: Option<&str>,
    config_storage: Option<&str>,
    anvil: &AnvilSettings,
) -> Result<Remote, String> {
    let usable = |v: Option<&str>| {
        v.map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_string)
    };
    let warehouse = usable(env_url)
        .or_else(|| usable(config_url))
        .or_else(|| anvil.warehouse_url.clone())
        .ok_or(
            "no Warehouse configured: set LOUPE_WAREHOUSE_URL (Warehouse with its base path, e.g. \
             https://host/warehouse), or `warehouse_url` in ~/.config/loupe/config.toml, or run loupe inside a \
             checkout whose .anvil.toml has `warehouse_url` under [docs]",
        )?;
    let storage = usable(env_storage)
        .or_else(|| usable(config_storage))
        .or_else(|| anvil.storage.clone())
        .unwrap_or_else(|| DEFAULT_STORAGE.to_string());
    Ok(Remote {
        warehouse: warehouse.trim_end_matches('/').to_string(),
        storage,
    })
}

/// `forge` or `forge@2026.11.08`: the book and, when given, the version.
///
/// # Errors
/// An empty name or version, or one with `/`, `..` or a leading `.`.
pub fn parse_spec(spec: &str) -> Result<(String, Option<String>), String> {
    let (name, version) = match spec.split_once('@') {
        Some((name, version)) => (name, Some(version)),
        None => (spec, None),
    };
    let safe = |part: &str| {
        !part.is_empty() && !part.contains('/') && !part.contains('\\') && !part.starts_with('.')
    };
    if !safe(name) {
        return Err(format!(
            "`{spec}` is not a book name (BOOK or BOOK@VERSION)"
        ));
    }
    if let Some(version) = version
        && !safe(version)
    {
        return Err(format!("`{spec}` has no usable version after `@`"));
    }
    Ok((name.to_string(), version.map(str::to_string)))
}

fn encode(text: &str) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~' | b'/') {
            out.push(char::from(b));
        } else {
            let _ = write!(out, "%{b:02X}");
        }
    }
    out
}

impl Remote {
    /// The URL of a file of the storage.
    #[must_use]
    pub fn file_url(&self, path: &str) -> String {
        format!(
            "{}/api/v1/files/{}/file?path={}",
            self.warehouse,
            encode(&self.storage),
            encode(path)
        )
    }

    /// The URL of the storage's listing.
    #[must_use]
    pub fn list_url(&self) -> String {
        format!("{}/api/v1/files/{}", self.warehouse, encode(&self.storage))
    }
}

/// A session with Warehouse: one token for every request.
struct Session<'a> {
    remote: &'a Remote,
    net: &'a dyn Net,
    token: Option<String>,
}

impl<'a> Session<'a> {
    fn start(remote: &'a Remote, net: &'a dyn Net) -> Result<Self, String> {
        let token = net.token().map_err(|e| {
            format!("cannot get a Warehouse token: {e}; run `loupe login` again (or set LOUPE_WAREHOUSE_TOKEN)")
        })?;
        Ok(Self { remote, net, token })
    }

    /// `GET`s `url`; `None` for a 404, an error naming what to do for a refusal.
    fn get(&self, url: &str) -> Result<Option<Vec<u8>>, String> {
        let (status, body) = self
            .net
            .get(url, self.token.as_deref())
            .map_err(|e| format!("cannot reach Warehouse at {}: {e}", self.remote.warehouse))?;
        let storage = &self.remote.storage;
        match status {
            200..=299 => Ok(Some(body)),
            404 => Ok(None),
            401 | 403 if self.token.is_none() => Err(format!(
                "the `{storage}` storage at {} needs credentials ({status}): run `loupe login` (or `anvil login` \
                 in a checkout), or set LOUPE_WAREHOUSE_TOKEN",
                self.remote.warehouse
            )),
            401 | 403 => Err(format!(
                "Warehouse refused the credentials for the `{storage}` storage ({status}): run `loupe login` \
                 again; reading needs the `warehouse:read` grant"
            )),
            _ => Err(format!(
                "Warehouse answered {status} for {url}: {}",
                String::from_utf8_lossy(&body)
                    .chars()
                    .take(200)
                    .collect::<String>()
            )),
        }
    }

    fn json(&self, path: &str) -> Result<Option<Value>, String> {
        let Some(body) = self.get(&self.remote.file_url(path))? else {
            return Ok(None);
        };
        let text = String::from_utf8(body).map_err(|_| format!("{path} is not text"))?;
        quench_serde::json::from_str(&text)
            .map(Some)
            .map_err(|e| format!("{path} is not JSON: {e}"))
    }
}

fn text(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

/// The cache of released books: `$XDG_CACHE_HOME/loupe` when that is absolute, else `~/.cache/loupe`.
#[must_use]
pub fn cache_root(xdg_cache_home: Option<&str>, home: Option<&str>) -> Option<PathBuf> {
    if let Some(xdg) = xdg_cache_home.filter(|x| Path::new(x).is_absolute()) {
        return Some(Path::new(xdg).join("loupe"));
    }
    home.filter(|h| !h.is_empty())
        .map(|h| Path::new(h).join(".cache").join("loupe"))
}

/// The marker written last when a version is unpacked: a directory without it is not trusted.
const COMPLETE: &str = ".loupe-complete";

/// A released book in the cache: where it is, and what it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opened {
    pub dir: PathBuf,
    pub book: String,
    pub version: String,
    /// Whether it was in the cache already (nothing was downloaded).
    pub cached: bool,
}

/// The cached copy of `book` at `version`, if it is complete.
fn cached(cache: &Path, book: &str, version: &str) -> Option<PathBuf> {
    let dir = cache.join(book).join(version);
    dir.join(COMPLETE).is_file().then_some(dir)
}

/// Opens `spec` (`BOOK` or `BOOK@VERSION`) from `remote`, through the cache at `cache`.
///
/// A version given and already cached opens without asking Warehouse. Otherwise the release's manifest says
/// which version and archive it is; a cached version opens as it is, else the archive is downloaded, checked
/// against its checksum, and its `_source/` unpacked.
///
/// # Errors
/// Warehouse cannot be reached or refuses, the book or version is unknown, the archive is damaged or has no
/// `_source/`, or the cache cannot be written.
pub fn open(spec: &str, remote: &Remote, net: &dyn Net, cache: &Path) -> Result<Opened, String> {
    let (book, version) = parse_spec(spec)?;
    if let Some(version) = &version
        && let Some(dir) = cached(cache, &book, version)
    {
        return Ok(Opened {
            dir,
            book,
            version: version.clone(),
            cached: true,
        });
    }
    let session = Session::start(remote, net)?;
    let storage = &remote.storage;
    let (manifest, archive_path) = match &version {
        None => {
            let latest = session.json(&format!("{book}/latest.json"))?.ok_or_else(|| {
                format!("no released book `{book}` in the `{storage}` storage (`loupe --book` lists them)")
            })?;
            let path = text(&latest, "archive_path").or_else(|| {
                let version = text(&latest, "version")?;
                Some(format!("{book}/{version}/{}", text(&latest, "archive")?))
            });
            (latest, path)
        }
        Some(version) => {
            let Some(manifest) = session.json(&format!("{book}/{version}/manifest.json"))? else {
                let latest = session
                    .json(&format!("{book}/latest.json"))?
                    .and_then(|l| text(&l, "version"));
                return Err(latest.map_or_else(
                    || format!("no released book `{book}` in the `{storage}` storage (`loupe --book` lists them)"),
                    |latest| format!("the book `{book}` has no version `{version}` (the latest is {latest})"),
                ));
            };
            let path = text(&manifest, "archive").map(|a| format!("{book}/{version}/{a}"));
            (manifest, path)
        }
    };
    let version = version
        .or_else(|| text(&manifest, "version"))
        .ok_or_else(|| format!("the manifest of `{book}` names no version"))?;
    if let Some(dir) = cached(cache, &book, &version) {
        return Ok(Opened {
            dir,
            book,
            version,
            cached: true,
        });
    }
    let old_release = || {
        format!(
            "the release {book} {version} has no Markdown sources (_source/): it was released before loupe \
             support; release the book again with a newer anvil"
        )
    };
    if manifest.get("sources").is_none() {
        return Err(old_release());
    }
    let archive_path =
        archive_path.ok_or_else(|| format!("the manifest of {book} {version} names no archive"))?;
    let archive = session
        .get(&remote.file_url(&archive_path))?
        .ok_or_else(|| {
            format!("the archive of {book} {version} is missing from the storage ({archive_path})")
        })?;
    if let Some(expected) = text(&manifest, "archive_sha256") {
        let actual = quench_core::hex::encode(quench_crypto::hash::sha256(&archive));
        if !actual.eq_ignore_ascii_case(&expected) {
            return Err(format!(
                "the archive of {book} {version} does not match its checksum: download it again"
            ));
        }
    }
    let sources =
        sources_of(&archive).map_err(|e| format!("the archive of {book} {version}: {e}"))?;
    if sources.is_empty() {
        return Err(old_release());
    }
    let dir = unpack(cache, &book, &version, &sources)?;
    Ok(Opened {
        dir,
        book,
        version,
        cached: false,
    })
}

/// The files under `_source/` of a release archive: (path below `_source/`, bytes).
///
/// # Errors
/// The archive is not a gzip-compressed tar, or a path in it climbs out.
pub fn sources_of(archive: &[u8]) -> Result<Vec<(String, Vec<u8>)>, String> {
    let tar = quench_archive::gzip::decompress(archive, MAX_UNPACKED)
        .map_err(|e| format!("cannot decompress it: {e}"))?;
    let mut out = Vec::new();
    for entry in quench_archive::tar::Reader::new(&tar) {
        let entry = entry.map_err(|e| format!("cannot read it: {e}"))?;
        if entry.kind != quench_archive::tar::EntryKind::File {
            continue;
        }
        let Some(rest) = entry.path.strip_prefix(SOURCES) else {
            continue;
        };
        let clean = Path::new(rest)
            .components()
            .all(|c| matches!(c, Component::Normal(_)));
        if !clean || rest.is_empty() {
            return Err(format!("the path {} climbs out of _source/", entry.path));
        }
        out.push((rest.to_string(), entry.data.to_vec()));
    }
    Ok(out)
}

/// Writes `files` into the cache as `<book>/<version>/`, through a scratch directory renamed into place, so a
/// version is either there whole or not at all.
fn unpack(
    cache: &Path,
    book: &str,
    version: &str,
    files: &[(String, Vec<u8>)],
) -> Result<PathBuf, String> {
    let parent = cache.join(book);
    let target = parent.join(version);
    let scratch = parent.join(format!(".{version}.partial-{}", std::process::id()));
    let fail = |e: std::io::Error, what: &Path| {
        format!("cannot write the cache at {}: {e}", what.display())
    };
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).map_err(|e| fail(e, &scratch))?;
    for (path, data) in files {
        let file = scratch.join(path);
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir).map_err(|e| fail(e, dir))?;
        }
        std::fs::write(&file, data).map_err(|e| fail(e, &file))?;
    }
    std::fs::write(scratch.join(COMPLETE), version).map_err(|e| fail(e, &scratch))?;
    // A copy left by an interrupted run, without its marker, is replaced.
    if target.is_dir() {
        std::fs::remove_dir_all(&target).map_err(|e| fail(e, &target))?;
    }
    std::fs::rename(&scratch, &target).map_err(|e| fail(e, &target))?;
    Ok(target)
}

/// A released book, as `loupe --book` lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed {
    pub book: String,
    pub version: Option<String>,
    pub title: Option<String>,
}

/// The books in the storage: its top-level directories (or, for a storage that lists files, the books that
/// have a `latest.json`), each with its latest version and title.
///
/// # Errors
/// Warehouse cannot be reached or refuses, or the listing is not JSON.
pub fn list(remote: &Remote, net: &dyn Net) -> Result<Vec<Listed>, String> {
    let session = Session::start(remote, net)?;
    let storage = &remote.storage;
    let body = session
        .get(&remote.list_url())?
        .ok_or_else(|| format!("Warehouse has no files storage `{storage}` (404)"))?;
    let listing: Value = quench_serde::json::from_str(&String::from_utf8_lossy(&body))
        .map_err(|e| format!("the listing of `{storage}` is not JSON: {e}"))?;
    let mut names: Vec<String> = listing
        .get("entries")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| {
                    let kind = entry.get("kind").and_then(Value::as_str)?;
                    match kind {
                        "directory" => entry
                            .get("name")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                        _ => entry
                            .get("path")
                            .and_then(Value::as_str)
                            .and_then(|p| p.strip_suffix("/latest.json"))
                            .filter(|b| !b.contains('/'))
                            .map(str::to_string),
                    }
                })
                .filter(|name| !name.starts_with('.'))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names.dedup();
    let mut out = Vec::new();
    for book in names {
        let latest = session.json(&format!("{book}/latest.json"))?;
        // A directory without a `latest.json` is not a released book.
        let Some(latest) = latest else { continue };
        out.push(Listed {
            version: text(&latest, "version"),
            title: text(&latest, "title"),
            book,
        });
    }
    Ok(out)
}

/// Where the credentials are looked for, and how the environment is read.
#[derive(Debug, Clone)]
pub struct Credentials {
    /// The working directory: `.loupe/secret.toml` or `.loupe.secret.toml` here or above, then anvil's.
    pub cwd: PathBuf,
    /// loupe's config directory, where `loupe login` stores its token.
    pub config_dir: Option<PathBuf>,
    /// `LOUPE_WAREHOUSE_TOKEN`.
    pub token: Option<String>,
    /// `ANVIL_APP_TOKEN` and `ANVIL_GATEHOUSE_URL`, as anvil reads them in a CI job.
    pub anvil_app: Option<(String, String)>,
}

impl Credentials {
    /// The stored application token: loupe's (here or above, then in its config directory), else anvil's.
    ///
    /// # Errors
    /// A secret file cannot be read.
    pub fn stored(&self) -> Result<Option<gatehouse_auth::Credentials>, String> {
        let load =
            |tool: &str, dir: &Path| gatehouse_auth::load(tool, dir).map_err(|e| e.to_string());
        if let Some(found) = load(TOOL, &self.cwd)? {
            return Ok(Some(found));
        }
        if let Some(dir) = &self.config_dir
            && let Some(found) = load(TOOL, dir)?
        {
            return Ok(Some(found));
        }
        if let Some((app_token, gatehouse_url)) = &self.anvil_app {
            return Ok(Some(gatehouse_auth::Credentials {
                gatehouse_url: gatehouse_url.clone(),
                app_token: app_token.clone(),
            }));
        }
        load("anvil", &self.cwd)
    }
}

/// The tool name of loupe's secret file (`.loupe.secret.toml`).
pub const TOOL: &str = "loupe";

/// Warehouse over HTTP, with the credentials found as [`Credentials`] says.
#[derive(Debug)]
pub struct Http {
    client: quench_client::blocking::Client,
    credentials: Credentials,
}

impl Http {
    /// # Errors
    /// The HTTP client cannot be built.
    pub fn new(credentials: Credentials) -> Result<Self, String> {
        let client = quench_client::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(|e| format!("cannot build the HTTP client: {e}"))?;
        Ok(Self {
            client,
            credentials,
        })
    }
}

impl Net for Http {
    fn get(&self, url: &str, token: Option<&str>) -> Result<(u16, Vec<u8>), String> {
        let mut request = self.client.get(url);
        if let Some(token) = token {
            request = request.bearer_auth(token);
        }
        let response = request.send().map_err(|e| e.to_string())?;
        let status = response.status().as_u16();
        let body = response.bytes().map_err(|e| e.to_string())?;
        Ok((status, body.to_vec()))
    }

    fn token(&self) -> Result<Option<String>, String> {
        if let Some(token) = &self.credentials.token {
            return Ok(Some(token.clone()));
        }
        self.credentials.stored()?.map_or(Ok(None), |stored| {
            gatehouse_auth::access_token(&stored)
                .map(Some)
                .map_err(|e| e.to_string())
        })
    }
}
