//! Building, reading and unpacking the archive.
//!
//! Reading never extracts through `tar`: entries are validated and held in
//! memory, so there is no code path in which an archive chooses where a byte
//! lands. [`Package::write_to`] is the only place that touches a directory, and
//! it only writes paths that already passed [`crate::path::validate`].

use crate::error::PackageError;
use crate::manifest::Manifest;
use crate::{MANIFEST_FILE, OVERLAY_FILE, SUMS_FILE, path, sha256_hex};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::Read;
use std::path::Path;

/// Zstd level for packing. Overlays are small text, so spend the CPU.
const ZSTD_LEVEL: i32 = 19;
/// Tar's per-entry header and padding, for sizing the decoded-stream cap.
const TAR_ENTRY_OVERHEAD: u64 = 1024;

/// What a reader will accept before refusing an archive as a bomb.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Files in the archive.
    pub max_entries: usize,
    /// Size of any one file once decompressed.
    pub max_file_bytes: u64,
    /// Sum of every file once decompressed.
    pub max_total_bytes: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_entries: 2000,
            max_file_bytes: 16 * 1024 * 1024,
            max_total_bytes: 128 * 1024 * 1024,
        }
    }
}

/// A validated package, held in memory: its manifest and every file, keyed by path.
#[derive(Debug, Clone, PartialEq)]
pub struct Package {
    /// The parsed `rivet.toml`.
    pub manifest: Manifest,
    /// Every file in the archive, `rivet.toml` and `SHA256SUMS` included.
    pub files: BTreeMap<String, Vec<u8>>,
}

impl Package {
    /// Reads a `tar.zst`, applying every check a registry needs before storing it.
    pub fn read(reader: impl Read, limits: &Limits) -> Result<Self, PackageError> {
        let decoder = zstd::stream::read::Decoder::new(reader)
            .map_err(|e| PackageError::Archive(format!("not a zstd stream: {e}")))?;
        // Entry sizes are checked against the limits individually; this is the
        // backstop for a stream that lies about them.
        let cap = limits
            .max_total_bytes
            .saturating_add((limits.max_entries as u64 + 2).saturating_mul(TAR_ENTRY_OVERHEAD));
        let mut archive = tar::Archive::new(decoder.take(cap));

        let mut files = BTreeMap::new();
        let mut total: u64 = 0;
        let entries = archive
            .entries()
            .map_err(|e| PackageError::Archive(e.to_string()))?;

        for entry in entries {
            let mut entry = entry.map_err(|e| PackageError::Archive(e.to_string()))?;
            let kind = entry.header().entry_type();
            if kind.is_dir() {
                continue;
            }
            let name = entry
                .path()
                .map_err(|e| PackageError::Archive(e.to_string()))?
                .to_str()
                .map(str::to_string)
                .ok_or_else(|| PackageError::Archive("a path is not valid UTF-8".to_string()))?;
            path::validate(&name)?;

            if !kind.is_file() {
                return Err(PackageError::UnsafePath(
                    name,
                    "only regular files are allowed (no links or devices)",
                ));
            }
            if files.len() >= limits.max_entries {
                return Err(PackageError::Limit(format!(
                    "more than {} files",
                    limits.max_entries
                )));
            }

            let size = entry
                .header()
                .size()
                .map_err(|e| PackageError::Archive(e.to_string()))?;
            if size > limits.max_file_bytes {
                return Err(PackageError::Limit(format!(
                    "`{name}` is larger than {} bytes",
                    limits.max_file_bytes
                )));
            }
            total = total.saturating_add(size);
            if total > limits.max_total_bytes {
                return Err(PackageError::Limit(format!(
                    "contents exceed {} bytes",
                    limits.max_total_bytes
                )));
            }

            let mut data = Vec::with_capacity(usize::try_from(size.min(1 << 20)).unwrap_or(0));
            entry.by_ref().take(size).read_to_end(&mut data)?;
            if files.insert(name.clone(), data).is_some() {
                return Err(PackageError::Archive(format!("`{name}` appears twice")));
            }
        }

        let manifest = Self::check(&files)?;
        Ok(Self { manifest, files })
    }

    /// The structural checks: required files, a valid manifest, matching sums.
    fn check(files: &BTreeMap<String, Vec<u8>>) -> Result<Manifest, PackageError> {
        for required in [MANIFEST_FILE, OVERLAY_FILE, SUMS_FILE] {
            if !files.contains_key(required) {
                return Err(PackageError::Missing(required));
            }
        }

        let manifest_text = std::str::from_utf8(&files[MANIFEST_FILE])
            .map_err(|_| PackageError::Manifest("rivet.toml is not UTF-8".to_string()))?;
        let manifest = Manifest::parse(manifest_text)?;

        let sums = parse_sums(&files[SUMS_FILE])?;
        for (file, data) in files {
            if file == SUMS_FILE {
                continue;
            }
            match sums.get(file) {
                None => {
                    return Err(PackageError::Checksum(format!(
                        "`{file}` is not listed in {SUMS_FILE}"
                    )));
                }
                Some(expected) if *expected != sha256_hex(data) => {
                    return Err(PackageError::Checksum(format!("`{file}` does not match")));
                }
                Some(_) => {}
            }
        }
        if let Some(extra) = sums.keys().find(|file| !files.contains_key(*file)) {
            return Err(PackageError::Checksum(format!(
                "{SUMS_FILE} lists `{extra}`, which the archive lacks"
            )));
        }
        Ok(manifest)
    }

    /// Writes every file under `dir`, which must not already hold them.
    pub fn write_to(&self, dir: &Path) -> Result<(), PackageError> {
        use std::io::Write;
        for (name, data) in &self.files {
            path::validate(name)?;
            let target = dir.join(name);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            // `create_new`: never follow or overwrite something already there.
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&target)?
                .write_all(data)?;
        }
        Ok(())
    }
}

/// `<hex>  <path>` per line, as `sha256sum` writes it.
fn parse_sums(data: &[u8]) -> Result<BTreeMap<String, String>, PackageError> {
    let text = std::str::from_utf8(data)
        .map_err(|_| PackageError::Checksum(format!("{SUMS_FILE} is not UTF-8")))?;
    let mut sums = BTreeMap::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let (digest, file) = line
            .split_once("  ")
            .filter(|(d, _)| d.len() == 64 && d.bytes().all(|b| b.is_ascii_hexdigit()))
            .ok_or_else(|| PackageError::Checksum(format!("malformed line `{line}`")))?;
        if sums
            .insert(file.to_string(), digest.to_ascii_lowercase())
            .is_some()
        {
            return Err(PackageError::Checksum(format!("`{file}` is listed twice")));
        }
    }
    Ok(sums)
}

/// Assembles a package. The manifest and `SHA256SUMS` are generated, so a
/// caller supplies only the overlay's own files.
#[derive(Debug)]
pub struct PackageBuilder {
    manifest: Manifest,
    files: BTreeMap<String, Vec<u8>>,
}

impl PackageBuilder {
    /// A builder for `manifest`, which is validated at [`Self::build`].
    #[must_use]
    pub const fn new(manifest: Manifest) -> Self {
        Self {
            manifest,
            files: BTreeMap::new(),
        }
    }

    /// Adds a file. `rivet.toml` and `SHA256SUMS` are reserved.
    pub fn add_file(
        &mut self,
        name: impl Into<String>,
        data: Vec<u8>,
    ) -> Result<&mut Self, PackageError> {
        let name = name.into();
        path::validate(&name)?;
        if name == MANIFEST_FILE || name == SUMS_FILE {
            return Err(PackageError::UnsafePath(name, "reserved file name"));
        }
        if self.files.insert(name.clone(), data).is_some() {
            return Err(PackageError::Archive(format!("`{name}` added twice")));
        }
        Ok(self)
    }

    /// The finished package, in memory.
    pub fn finish(self) -> Result<Package, PackageError> {
        if !self.files.contains_key(OVERLAY_FILE) {
            return Err(PackageError::Missing(OVERLAY_FILE));
        }
        let mut files = self.files;
        files.insert(
            MANIFEST_FILE.to_string(),
            self.manifest.to_toml()?.into_bytes(),
        );

        let mut sums = String::new();
        for (name, data) in &files {
            let _ = writeln!(sums, "{}  {name}", sha256_hex(data));
        }
        files.insert(SUMS_FILE.to_string(), sums.into_bytes());

        Ok(Package {
            manifest: self.manifest,
            files,
        })
    }

    /// The `tar.zst` bytes. Byte-identical for identical input: entries are
    /// sorted, and owner, mode and mtime are fixed.
    pub fn build(self) -> Result<Vec<u8>, PackageError> {
        self.finish()?.to_bytes()
    }
}

impl Package {
    /// Serialises to a deterministic `tar.zst`.
    pub fn to_bytes(&self) -> Result<Vec<u8>, PackageError> {
        let mut builder = tar::Builder::new(Vec::new());
        // `BTreeMap` iterates sorted, which is what makes the output stable.
        for (name, data) in &self.files {
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_uid(0);
            header.set_gid(0);
            header.set_mtime(0);
            header.set_entry_type(tar::EntryType::Regular);
            builder.append_data(&mut header, name, data.as_slice())?;
        }
        let tar_bytes = builder.into_inner()?;
        Ok(zstd::stream::encode_all(tar_bytes.as_slice(), ZSTD_LEVEL)?)
    }
}
