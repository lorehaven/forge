//! One release of one book, unpacked in memory.

use quench_core::bytes::Bytes;
use quench_serde::Value;
use std::collections::HashMap;

/// Most a release may unpack to.
const MAX_UNPACKED_BYTES: usize = 512 * 1024 * 1024;

/// A file of a book and what it is served as.
#[derive(Debug, Clone)]
pub struct File {
    pub content_type: &'static str,
    pub body: Bytes,
}

/// What a request path comes to.
#[derive(Debug)]
pub enum Found<'a> {
    File(&'a File),
    /// A directory asked for without its trailing slash: go there, so relative links resolve.
    Slash,
    Missing,
}

#[derive(Debug, Clone)]
pub struct Book {
    pub name: String,
    pub title: String,
    pub version: String,
    pub built: String,
    files: HashMap<String, File>,
}

impl Book {
    /// Unpacks `archive` (a gzip-compressed tar of the built site) described by `latest` (the manifest anvil wrote).
    ///
    /// A book built without a `baseurl` has every link written from the site root (`/assets/...`); those are
    /// rewritten to start with `prefix`, the path the book is served under. A book built with a `baseurl` is
    /// taken as it is: it was built for wherever it is served.
    ///
    /// # Errors
    ///
    /// When the archive does not match the checksum in the manifest, cannot be unpacked, or holds no files.
    pub fn from_release(
        name: &str,
        latest: &Value,
        archive: &[u8],
        prefix: &str,
    ) -> Result<Self, String> {
        let text = |key: &str| latest.get(key).and_then(Value::as_str).map(str::to_string);

        if let Some(expected) = text("archive_sha256") {
            let actual = quench_core::hex::encode(quench_crypto::hash::sha256(archive));
            if !actual.eq_ignore_ascii_case(&expected) {
                return Err(format!(
                    "the archive does not match its checksum ({actual} != {expected})"
                ));
            }
        }

        let rewrite = text("baseurl").is_none_or(|baseurl| baseurl.trim_matches('/').is_empty());
        let tar = quench_archive::gzip::decompress(archive, MAX_UNPACKED_BYTES)
            .map_err(|e| format!("cannot decompress the archive: {e}"))?;

        let mut files = HashMap::new();
        for entry in quench_archive::tar::Reader::new(&tar) {
            let entry = entry.map_err(|e| format!("cannot read the archive: {e}"))?;
            if entry.kind != quench_archive::tar::EntryKind::File {
                continue;
            }
            let Some(path) = clean(&entry.path) else {
                continue;
            };
            let content_type = content_type(&path);
            let body = if rewrite {
                rewrite_root_urls(entry.data, &path, prefix)
            } else {
                Bytes::copy_from_slice(entry.data)
            };
            files.insert(path, File { content_type, body });
        }
        if files.is_empty() {
            return Err("the archive holds no files".to_string());
        }

        Ok(Self {
            name: name.to_string(),
            title: text("title").unwrap_or_else(|| name.to_string()),
            version: text("version").unwrap_or_default(),
            built: text("built").unwrap_or_default(),
            files,
        })
    }

    /// A book from files already in hand (tests, mostly).
    #[must_use]
    pub fn from_files(name: &str, version: &str, files: &[(&str, &str)]) -> Self {
        Self {
            name: name.to_string(),
            title: name.to_string(),
            version: version.to_string(),
            built: String::new(),
            files: files
                .iter()
                .map(|(path, body)| {
                    (
                        (*path).to_string(),
                        File {
                            content_type: content_type(path),
                            body: Bytes::copy_from_slice(body.as_bytes()),
                        },
                    )
                })
                .collect(),
        }
    }

    #[must_use]
    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    /// The file for a path below the book (`""` and `"cli/"` are directories, served as their `index.html`).
    #[must_use]
    pub fn find(&self, path: &str) -> Found<'_> {
        let path = path.trim_start_matches('/');
        if path.is_empty() || path.ends_with('/') {
            return self
                .files
                .get(&format!("{path}index.html"))
                .map_or(Found::Missing, Found::File);
        }
        if let Some(file) = self.files.get(path) {
            return Found::File(file);
        }
        if self.files.contains_key(&format!("{path}/index.html")) {
            return Found::Slash;
        }
        Found::Missing
    }

    /// The book's own `404.html`, if the site has one.
    #[must_use]
    pub fn not_found_page(&self) -> Option<&File> {
        self.files.get("404.html")
    }
}

/// A path from an archive, relative and without `.` or `..` parts; `None` for one that would climb out.
#[must_use]
pub fn clean(path: &str) -> Option<String> {
    let mut parts = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => return None,
            part => parts.push(part),
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// What a file is served as, by its extension.
#[must_use]
pub fn content_type(path: &str) -> &'static str {
    let extension = path
        .rsplit_once('.')
        .map_or("", |(_, ext)| ext)
        .to_ascii_lowercase();
    match extension.as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" | "map" | "webmanifest" => "application/json; charset=utf-8",
        "xml" | "xsl" => "application/xml; charset=utf-8",
        "txt" | "md" | "yaml" | "yml" | "toml" => "text/plain; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "pdf" => "application/pdf",
        _ => "application/octet-stream",
    }
}

/// The text a file's root-absolute links are found in, by what kind of file it is: the attributes that hold
/// them in HTML, `url(...)` in CSS, the quoted paths the just-the-docs script and its search index use.
fn needles(path: &str) -> &'static [&'static str] {
    let extension = path
        .rsplit_once('.')
        .map_or("", |(_, ext)| ext)
        .to_ascii_lowercase();
    match extension.as_str() {
        "html" | "htm" => &["href=\"/", "src=\"/", "action=\"/", "href='/", "src='/"],
        "css" => &["url(/", "url(\"/", "url('/"],
        "js" => &["'/assets/", "\"/assets/"],
        "json" if path.ends_with("search-data.json") => &["\"url\": \"/", "\"url\":\"/"],
        _ => &[],
    }
}

/// Whether the end of `html` is inside a tag that has been opened and not yet closed.
fn inside_tag(html: &str) -> bool {
    match (html.rfind('<'), html.rfind('>')) {
        (Some(open), Some(close)) => open > close,
        (Some(_), None) => true,
        _ => false,
    }
}

/// `data` with every root-absolute link (`href="/x"`) pointing below `prefix` instead (`href="/forge/x"`).
/// A protocol-relative link (`href="//host/x"`) is left alone, and so is anything that is not text.
#[must_use]
pub fn rewrite_root_urls(data: &[u8], path: &str, prefix: &str) -> Bytes {
    let needles = needles(path);
    let prefix = prefix.trim_end_matches('/');
    let Ok(text) = std::str::from_utf8(data) else {
        return Bytes::copy_from_slice(data);
    };
    if needles.is_empty() || prefix.is_empty() {
        return Bytes::copy_from_slice(data);
    }

    let markup = std::path::Path::new(path)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("html") || ext.eq_ignore_ascii_case("htm"));
    let mut out = String::with_capacity(text.len() + 1024);
    let mut rest = text;
    loop {
        let next = needles
            .iter()
            .filter_map(|needle| rest.find(needle).map(|at| (at, *needle)))
            .min_by_key(|(at, _)| *at);
        let Some((at, needle)) = next else {
            out.push_str(rest);
            break;
        };
        // The prefix goes in at the first slash of the needle, the one that makes the link root-absolute.
        let slash = at + needle.find('/').unwrap_or(0);
        out.push_str(&rest[..slash]);
        // In a page only a real tag's attribute is a link; prose that quotes `href="/x"` is left as written.
        let is_link = !markup || inside_tag(&out);
        if is_link && !rest[slash + 1..].starts_with('/') {
            out.push_str(prefix);
        }
        rest = &rest[slash..];
        // `rest` now starts at that slash, which no needle matches at its start, so the scan moves on.
    }
    Bytes::from(out.into_bytes())
}
