//! Checks overlay `image:` lines against their registries for newer tags,
//! and - with `--update` - rewrites the deployment templates in place.
//!
//! A port of the `check_image_updates.py` script that used to live next to
//! the overlay repo: the check now ships with riveter itself rather than as
//! a separate script an overlay repo has to carry around.

use anyhow::{Context as _, Result, bail};
use base64::Engine as _;
use regex::Regex;
use reqwest::blocking::Client;
use reqwest::header::{AUTHORIZATION, HeaderValue};
use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

/// Tags nobody should be compared against or offered as an update - they
/// name a moving target, not a version.
const FLOATING_TAGS: &[&str] = &[
    "latest", "stable", "edge", "main", "master", "dev", "nightly",
];

static IMAGE_LINE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?P<indent>\s*)image:\s*(?P<image>\S+)\s*$").unwrap());

static VERSION_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?P<prefix>[A-Za-z._-]*?)(?P<version>\d+(?:[._-]\d+)*)(?P<suffix>[A-Za-z._-]*)$")
        .unwrap()
});

static LINK_NEXT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"<([^>]+)>;\s*rel="next""#).unwrap());

/// A parsed `registry/repository:tag` reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageRef {
    pub original: String,
    pub registry: String,
    pub repository: String,
    pub tag: String,
}

impl ImageRef {
    /// `repository`, prefixed with the registry unless it is Docker Hub -
    /// Docker Hub images are written bare (`redis`, not
    /// `registry-1.docker.io/library/redis`).
    fn display_repository(&self) -> String {
        if self.registry == "registry-1.docker.io" {
            self.repository.clone()
        } else {
            format!("{}/{}", self.registry, self.repository)
        }
    }

    fn with_tag(&self, tag: &str) -> String {
        format!("{}:{tag}", self.display_repository())
    }
}

/// Parses `value` as an image reference, applying the same registry/tag
/// inference Docker itself uses: no dot, colon or `localhost` in the first
/// path segment means Docker Hub, and a bare Docker Hub repository name gets
/// `library/` inserted.
fn parse_image_ref(value: &str) -> Result<ImageRef, &'static str> {
    if value.contains('@') {
        return Err("digest-pinned images are not supported");
    }

    let last_segment = value.rsplit('/').next().unwrap_or(value);
    if !last_segment.contains(':') {
        return Err("image has no explicit tag");
    }

    let (name_without_tag, tag) = value
        .rsplit_once(':')
        .expect("checked above: last segment contains ':'");

    let mut parts = name_without_tag.splitn(2, '/');
    let first = parts.next().unwrap_or_default();
    let rest = parts.next();

    let (registry, repository) =
        if first.contains('.') || first.contains(':') || first == "localhost" {
            (first.to_string(), rest.unwrap_or_default().to_string())
        } else {
            (
                "registry-1.docker.io".to_string(),
                name_without_tag.to_string(),
            )
        };

    let registry = if registry == "docker.io" {
        "registry-1.docker.io".to_string()
    } else {
        registry
    };

    let repository = if registry == "registry-1.docker.io" && !repository.contains('/') {
        format!("library/{repository}")
    } else {
        repository
    };

    if repository.is_empty() {
        return Err("image repository is empty");
    }

    Ok(ImageRef {
        original: value.to_string(),
        registry,
        repository,
        tag: tag.to_string(),
    })
}

/// One `image:` line found in an overlay template.
#[derive(Debug, Clone)]
pub struct ImageOccurrence {
    pub path: PathBuf,
    pub line_number: usize,
    pub image: ImageRef,
}

fn location(occurrence: &ImageOccurrence) -> String {
    format!("{}:{}", occurrence.path.display(), occurrence.line_number)
}

/// Walks `overlays_dir` for `deployment*.yaml.j2` templates and collects every
/// `image:` line in them.
///
/// A line that does not parse as an image reference is reported to stderr
/// and skipped rather than failing the scan - one malformed line should not
/// block checking every other image.
pub fn discover_images(overlays_dir: &Path) -> Result<Vec<ImageOccurrence>> {
    let mut paths: Vec<PathBuf> = walkdir::WalkDir::new(overlays_dir)
        .into_iter()
        .filter_map(std::result::Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .map(walkdir::DirEntry::into_path)
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("deployment") && name.ends_with(".yaml.j2"))
        })
        .collect();
    paths.sort();

    let mut occurrences = Vec::new();
    for path in paths {
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        for (idx, line) in text.lines().enumerate() {
            let Some(captures) = IMAGE_LINE_RE.captures(line) else {
                continue;
            };
            let value = captures["image"].trim_matches(['"', '\'']);
            match parse_image_ref(value) {
                Ok(image) => occurrences.push(ImageOccurrence {
                    path: path.clone(),
                    line_number: idx + 1,
                    image,
                }),
                Err(reason) => {
                    eprintln!("skip {}:{}: {value} ({reason})", path.display(), idx + 1);
                }
            }
        }
    }

    Ok(occurrences)
}

#[derive(Debug, Clone)]
struct RegistryCredentials {
    username: String,
    password: String,
}

impl RegistryCredentials {
    fn basic_auth_header(&self) -> String {
        let token = base64::engine::general_purpose::STANDARD
            .encode(format!("{}:{}", self.username, self.password));
        format!("Basic {token}")
    }
}

/// Registry credentials, keyed by normalized registry host - `"*"` is the
/// fallback used when nothing more specific matches.
#[derive(Debug, Default)]
pub struct RegistryAuth {
    credentials: BTreeMap<String, RegistryCredentials>,
}

impl RegistryAuth {
    /// Collects credentials in ascending precedence: Docker's own config,
    /// then `RIVETER_REGISTRY_AUTH`/`RIVETER_REGISTRY_USERNAME`+`_PASSWORD`,
    /// then explicit `--registry-auth` flags.
    #[must_use]
    pub fn from_sources(cli_auth: &[String]) -> Self {
        let mut credentials = docker_config_credentials();

        for (registry, creds) in
            parse_registry_auth_env(&std::env::var("RIVETER_REGISTRY_AUTH").unwrap_or_default())
        {
            credentials.insert(registry, creds);
        }

        if let (Ok(username), Ok(password)) = (
            std::env::var("RIVETER_REGISTRY_USERNAME"),
            std::env::var("RIVETER_REGISTRY_PASSWORD"),
        ) {
            credentials.insert("*".to_string(), RegistryCredentials { username, password });
        }

        for item in cli_auth {
            if let Some((registry, creds)) = parse_registry_auth_item(item) {
                credentials.insert(registry, creds);
            }
        }

        Self { credentials }
    }

    fn get(&self, registry: &str) -> Option<&RegistryCredentials> {
        self.credentials
            .get(registry)
            .or_else(|| self.credentials.get("*"))
    }

    fn basic_header(&self, registry: &str) -> Option<String> {
        self.get(registry)
            .map(RegistryCredentials::basic_auth_header)
    }
}

fn parse_username_password(value: &str) -> Option<RegistryCredentials> {
    let (username, password) = value.split_once(':')?;
    if username.is_empty() {
        return None;
    }
    Some(RegistryCredentials {
        username: username.to_string(),
        password: password.to_string(),
    })
}

fn parse_registry_auth_item(item: &str) -> Option<(String, RegistryCredentials)> {
    let (registry, raw_credentials) = item.trim().split_once('=')?;
    if registry.is_empty() {
        return None;
    }
    let credentials = parse_username_password(raw_credentials)?;
    Some((normalize_registry_key(registry), credentials))
}

fn parse_registry_auth_env(value: &str) -> BTreeMap<String, RegistryCredentials> {
    value
        .split(';')
        .filter_map(parse_registry_auth_item)
        .collect()
}

fn normalize_registry_key(value: &str) -> String {
    if value == "*" {
        return value.to_string();
    }
    let without_scheme = value.split_once("://").map_or(value, |(_, rest)| rest);
    let registry = without_scheme.trim_end_matches('/');
    if matches!(
        registry,
        "docker.io" | "index.docker.io" | "https://index.docker.io/v1"
    ) {
        "registry-1.docker.io".to_string()
    } else {
        registry.to_string()
    }
}

fn docker_config_credentials() -> BTreeMap<String, RegistryCredentials> {
    let config_path = std::env::var("DOCKER_CONFIG").map_or_else(
        |_| {
            dirs_home()
                .unwrap_or_default()
                .join(".docker")
                .join("config.json")
        },
        |dir| Path::new(&dir).join("config.json"),
    );

    let Ok(text) = std::fs::read_to_string(&config_path) else {
        return BTreeMap::new();
    };
    let Ok(data) = serde_json::from_str::<serde_json::Value>(&text) else {
        return BTreeMap::new();
    };
    let Some(auths) = data.get("auths").and_then(serde_json::Value::as_object) else {
        return BTreeMap::new();
    };

    let mut credentials = BTreeMap::new();
    for (raw_registry, raw_auth) in auths {
        let registry = normalize_registry_key(raw_registry);
        let username = raw_auth.get("username").and_then(serde_json::Value::as_str);
        let password = raw_auth.get("password").and_then(serde_json::Value::as_str);
        if let (Some(username), Some(password)) = (username, password) {
            credentials.insert(
                registry,
                RegistryCredentials {
                    username: username.to_string(),
                    password: password.to_string(),
                },
            );
            continue;
        }

        let Some(encoded) = raw_auth.get("auth").and_then(serde_json::Value::as_str) else {
            continue;
        };
        let Ok(decoded) = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        else {
            continue;
        };
        if let Some(parsed) = parse_username_password(&decoded) {
            credentials.insert(registry, parsed);
        }
    }
    credentials
}

/// Percent-encodes `repository` for use in a URL path, leaving `/` alone -
/// a repository is a sequence of path segments, not one opaque piece, and a
/// registry sees `foo%2Fbar` as a different repository than `foo/bar`.
fn encode_repository_path(repository: &str) -> String {
    repository
        .split('/')
        .map(urlencoding::encode)
        .collect::<Vec<_>>()
        .join("/")
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// Why fetching a repository's tags failed - transport failures get a retry
/// over plain HTTP (this estate's own registry serves that way), an auth
/// denial does not, since the registry is reachable and simply said no.
#[derive(Debug)]
enum FetchError {
    Unreachable(String),
    Denied(String),
    Other(String),
}

impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unreachable(msg) | Self::Denied(msg) | Self::Other(msg) => f.write_str(msg),
        }
    }
}

fn docker_hub_tags(client: &Client, repository: &str) -> Result<Vec<String>, FetchError> {
    let mut tags = Vec::new();
    let mut url = format!(
        "https://hub.docker.com/v2/repositories/{}/tags?page_size=100",
        encode_repository_path(repository)
    );

    loop {
        let response = client
            .get(&url)
            .send()
            .map_err(|err| FetchError::Unreachable(format!("failed to reach {url}: {err}")))?;
        let status = response.status();
        let body: serde_json::Value = response
            .json()
            .map_err(|err| FetchError::Other(format!("unexpected Docker Hub response: {err}")))?;
        if !status.is_success() {
            return Err(FetchError::Other(format!("HTTP {status} for {url}")));
        }

        if let Some(results) = body.get("results").and_then(serde_json::Value::as_array) {
            for item in results {
                if let Some(name) = item.get("name").and_then(serde_json::Value::as_str) {
                    tags.push(name.to_string());
                }
            }
        }

        match body.get("next").and_then(serde_json::Value::as_str) {
            Some(next) => url = next.to_string(),
            None => break,
        }
    }

    Ok(tags)
}

/// Requests a Bearer token from the realm a `WWW-Authenticate` challenge
/// names, per the Docker Registry token-auth protocol.
fn bearer_token(
    client: &Client,
    www_authenticate: &str,
    basic_auth_header: Option<&str>,
    default_scope: &str,
) -> Option<String> {
    let challenge = www_authenticate.strip_prefix("Bearer ")?;
    let mut realm = None;
    let mut service = None;
    let mut scope = None;
    for part in challenge.split(',') {
        let (key, value) = part.trim().split_once('=')?;
        let value = value.trim_matches('"');
        match key {
            "realm" => realm = Some(value.to_string()),
            "service" => service = Some(value.to_string()),
            "scope" => scope = Some(value.to_string()),
            _ => {}
        }
    }
    let realm = realm?;
    let scope = scope.unwrap_or_else(|| default_scope.to_string());

    let mut request = client.get(&realm).query(&[("scope", scope.as_str())]);
    if let Some(service) = &service {
        request = request.query(&[("service", service.as_str())]);
    }
    if let Some(header) = basic_auth_header {
        request = request.header(AUTHORIZATION, HeaderValue::from_str(header).ok()?);
    }

    let data: serde_json::Value = request.send().ok()?.json().ok()?;
    data.get("token")
        .or_else(|| data.get("access_token"))
        .and_then(serde_json::Value::as_str)
        .map(std::string::ToString::to_string)
}

/// Lists tags from a Docker Registry HTTP API V2 endpoint, handling the
/// Bearer challenge/token dance and falling back to Basic auth when a
/// registry accepts that instead.
fn registry_v2_tags(
    client: &Client,
    image: &ImageRef,
    auth: &RegistryAuth,
    scheme: &str,
) -> Result<Vec<String>, FetchError> {
    let mut tags = Vec::new();
    let mut next_url = Some(format!(
        "{scheme}://{}/v2/{}/tags/list?n=100",
        image.registry,
        encode_repository_path(&image.repository)
    ));
    let mut auth_header: Option<String> = None;

    while let Some(url) = next_url.take() {
        let mut request = client.get(&url);
        if let Some(header) = &auth_header {
            request = request.header(
                AUTHORIZATION,
                HeaderValue::from_str(header)
                    .map_err(|err| FetchError::Other(format!("invalid auth header: {err}")))?,
            );
        }
        let response = request
            .send()
            .map_err(|err| FetchError::Unreachable(format!("failed to reach {url}: {err}")))?;

        if response.status() == reqwest::StatusCode::UNAUTHORIZED
            || response.status() == reqwest::StatusCode::FORBIDDEN
        {
            let challenge = response
                .headers()
                .get(reqwest::header::WWW_AUTHENTICATE)
                .and_then(|value| value.to_str().ok())
                .unwrap_or_default()
                .to_string();
            let basic = auth.basic_header(&image.registry);
            let default_scope = format!("repository:{}:pull", image.repository);

            let token = bearer_token(client, &challenge, basic.as_deref(), &default_scope);
            let candidate = token
                .map(|token| format!("Bearer {token}"))
                .or_else(|| basic.clone());

            let Some(candidate) = candidate else {
                return Err(FetchError::Denied(format!(
                    "registry {} requires authentication",
                    image.registry
                )));
            };

            let mut retry = client.get(&url);
            retry = retry.header(
                AUTHORIZATION,
                HeaderValue::from_str(&candidate)
                    .map_err(|err| FetchError::Other(format!("invalid auth header: {err}")))?,
            );
            let retried = retry
                .send()
                .map_err(|err| FetchError::Unreachable(format!("failed to reach {url}: {err}")))?;
            if !retried.status().is_success() {
                return Err(FetchError::Denied(format!(
                    "registry {} denied access to {}: HTTP {}",
                    image.registry,
                    image.repository,
                    retried.status()
                )));
            }
            auth_header = Some(candidate);
            next_url = extract_next(retried, &mut tags)?;
            continue;
        }

        if !response.status().is_success() {
            return Err(FetchError::Other(format!(
                "HTTP {} for {url}",
                response.status()
            )));
        }
        next_url = extract_next(response, &mut tags)?;
    }

    Ok(tags)
}

fn extract_next(
    response: reqwest::blocking::Response,
    tags: &mut Vec<String>,
) -> Result<Option<String>, FetchError> {
    let requested_url = response.url().clone();
    let link = response
        .headers()
        .get(reqwest::header::LINK)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let body: serde_json::Value = response
        .json()
        .map_err(|err| FetchError::Other(format!("unexpected registry response: {err}")))?;

    if let Some(values) = body.get("tags").and_then(serde_json::Value::as_array) {
        tags.extend(
            values
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(std::string::ToString::to_string),
        );
    }

    Ok(LINK_NEXT_RE
        .captures(&link)
        .and_then(|c| c.get(1))
        .map(|next| {
            requested_url
                .join(next.as_str())
                .map_or_else(|_| next.as_str().to_string(), |joined| joined.to_string())
        }))
}

fn list_tags(
    client: &Client,
    image: &ImageRef,
    auth: &RegistryAuth,
) -> Result<Vec<String>, FetchError> {
    if image.registry == "registry-1.docker.io" {
        return docker_hub_tags(client, &image.repository);
    }
    match registry_v2_tags(client, image, auth, "https") {
        Err(FetchError::Unreachable(_)) => registry_v2_tags(client, image, auth, "http"),
        other => other,
    }
}

/// `(prefix, numeric components, suffix)` for a tag that looks versioned,
/// e.g. `v1.2.3-alpine` -> `("v", [1, 2, 3], "-alpine")`. `None` for anything
/// that does not contain a run of dot/dash/underscore-separated numbers.
fn version_key(tag: &str) -> Option<(String, Vec<u64>, String)> {
    let captures = VERSION_RE.captures(tag)?;
    let prefix = captures["prefix"].to_string();
    let suffix = captures["suffix"].to_string();
    let numbers = captures["version"]
        .split(['.', '_', '-'])
        .map(str::parse::<u64>)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    Some((prefix, numbers, suffix))
}

/// The newest tag that is a plausible upgrade of `current`: same prefix and
/// suffix, at least as many version components, and not a jump across a
/// three-digit epoch boundary (`9.x` -> `100.x` reads as a different
/// versioning scheme, not a newer version).
fn newest_compatible_tag(current: &str, tags: &[String]) -> Option<String> {
    if FLOATING_TAGS.contains(&current) {
        return None;
    }
    let (current_prefix, current_numbers, current_suffix) = version_key(current)?;

    tags.iter()
        .filter_map(|tag| {
            let (prefix, numbers, suffix) = version_key(tag)?;
            if prefix != current_prefix || suffix != current_suffix {
                return None;
            }
            if numbers.len() < current_numbers.len() {
                return None;
            }
            if numbers.len() > current_numbers.len().max(3) {
                return None;
            }
            if current_numbers[0] < 100 && numbers[0] >= 100 {
                return None;
            }
            Some((numbers, tag.clone()))
        })
        .max()
        .map(|(_, tag)| tag)
}

#[derive(Debug)]
pub struct UpdateCandidate {
    occurrence: ImageOccurrence,
    newest_tag: String,
}

#[derive(Debug)]
enum ScanMessage {
    Skip {
        occurrence: ImageOccurrence,
        detail: String,
    },
    Error {
        occurrence: ImageOccurrence,
        detail: String,
    },
}

/// Checks every discovered image against its registry, reusing one tag
/// listing per `(registry, repository)` pair across occurrences that share
/// it.
fn find_updates(
    occurrences: Vec<ImageOccurrence>,
    auth: &RegistryAuth,
) -> (Vec<UpdateCandidate>, Vec<ScanMessage>) {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap_or_else(|_| Client::new());

    let mut updates = Vec::new();
    let mut messages = Vec::new();
    let mut tag_cache: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();

    for occurrence in occurrences {
        let image = &occurrence.image;
        if FLOATING_TAGS.contains(&image.tag.as_str()) {
            messages.push(ScanMessage::Skip {
                detail: format!("uses floating tag {:?}", image.tag),
                occurrence,
            });
            continue;
        }

        let cache_key = (image.registry.clone(), image.repository.clone());
        let tags = if let Some(cached) = tag_cache.get(&cache_key) {
            cached.clone()
        } else {
            match list_tags(&client, image, auth) {
                Ok(tags) => {
                    tag_cache.insert(cache_key, tags.clone());
                    tags
                }
                Err(err) => {
                    messages.push(ScanMessage::Error {
                        detail: err.to_string(),
                        occurrence,
                    });
                    continue;
                }
            }
        };

        if let Some(newest) = newest_compatible_tag(&image.tag, &tags)
            && newest != image.tag
        {
            updates.push(UpdateCandidate {
                occurrence,
                newest_tag: newest,
            });
        }
    }

    (updates, messages)
}

/// Rewrites each affected template's `image:` lines to the newest tag found.
/// Only unquoted `image:` lines are rewritten - the same limitation the
/// script this was ported from had, and nothing in this estate's overlays
/// quotes an image value.
fn apply_updates(updates: &[UpdateCandidate]) -> Result<()> {
    let mut by_path: BTreeMap<&Path, BTreeMap<&str, String>> = BTreeMap::new();
    for update in updates {
        by_path
            .entry(update.occurrence.path.as_path())
            .or_default()
            .insert(
                &update.occurrence.image.original,
                update.occurrence.image.with_tag(&update.newest_tag),
            );
    }

    for (path, replacements) in by_path {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let rewritten: String = text
            .lines()
            .map(|line| {
                let Some(captures) = IMAGE_LINE_RE.captures(line) else {
                    return line.to_string();
                };
                let indent = &captures["indent"];
                let value = &captures["image"];
                replacements.get(value).map_or_else(
                    || line.to_string(),
                    |new_value| format!("{indent}image: {new_value}"),
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        // `Lines` drops a trailing newline; every overlay template in this
        // repo ends with one, so put it back.
        let rewritten = if text.ends_with('\n') && !rewritten.ends_with('\n') {
            format!("{rewritten}\n")
        } else {
            rewritten
        };
        std::fs::write(path, rewritten)
            .with_context(|| format!("failed to write {}", path.display()))?;
    }

    Ok(())
}

fn print_rows(title: &str, rows: &[[String; 4]]) {
    if rows.is_empty() {
        return;
    }

    let widths = [0, 1, 2, 3].map(|col| rows.iter().map(|row| row[col].len()).max().unwrap_or(0));
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let _ = writeln!(out, "{title}");
    for row in rows {
        if writeln!(
            out,
            "  {:<w0$}  {:<w1$}  {:<w2$}  {}",
            row[0],
            row[1],
            row[2],
            row[3],
            w0 = widths[0],
            w1 = widths[1],
            w2 = widths[2],
        )
        .is_err()
        {
            break;
        }
    }
    let _ = writeln!(out);
}

fn print_results(updates: &[UpdateCandidate], messages: &[ScanMessage]) {
    let skip_rows: Vec<[String; 4]> = messages
        .iter()
        .filter_map(|message| match message {
            ScanMessage::Skip { occurrence, detail } => Some([
                "skip".to_string(),
                location(occurrence),
                occurrence.image.original.clone(),
                detail.clone(),
            ]),
            ScanMessage::Error { .. } => None,
        })
        .collect();
    let error_rows: Vec<[String; 4]> = messages
        .iter()
        .filter_map(|message| match message {
            ScanMessage::Error { occurrence, detail } => Some([
                "error".to_string(),
                location(occurrence),
                occurrence.image.original.clone(),
                detail.clone(),
            ]),
            ScanMessage::Skip { .. } => None,
        })
        .collect();
    let update_rows: Vec<[String; 4]> = updates
        .iter()
        .map(|update| {
            [
                "update".to_string(),
                location(&update.occurrence),
                update.occurrence.image.original.clone(),
                update.occurrence.image.with_tag(&update.newest_tag),
            ]
        })
        .collect();

    print_rows("Skipped", &skip_rows);
    print_rows("Errors", &error_rows);
    print_rows("Updates", &update_rows);
}

/// Entry point for `riveter images`.
///
/// Scans `overlays_dir` for deployment image tags, checks each against its
/// registry, and either reports what is available or - with `apply` -
/// rewrites the templates to the newest compatible tag found.
pub fn check_image_updates(overlays_dir: &Path, apply: bool, cli_auth: &[String]) -> Result<()> {
    dotenvy::dotenv().ok();

    let auth = RegistryAuth::from_sources(cli_auth);
    let occurrences = discover_images(overlays_dir)?;
    let (updates, messages) = find_updates(occurrences, &auth);

    print_results(&updates, &messages);

    if updates.is_empty() {
        quench_cli::prelude::print_status(
            quench_cli::prelude::Tone::Success,
            "ok",
            "no updates found",
        );
        return Ok(());
    }

    if apply {
        let count = updates.len();
        apply_updates(&updates)?;
        quench_cli::prelude::print_status(
            quench_cli::prelude::Tone::Success,
            "ok",
            &format!("updated {count} image tag(s)"),
        );
    } else {
        quench_cli::prelude::print_status(
            quench_cli::prelude::Tone::Warn,
            "warn",
            &format!(
                "found {} update(s); re-run with --update to apply",
                updates.len()
            ),
        );
    }

    if !messages
        .iter()
        .any(|m| matches!(m, ScanMessage::Error { .. }))
    {
        return Ok(());
    }
    bail!("one or more images could not be checked; see the Errors table above");
}
