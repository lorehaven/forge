//! The covers and the artists' pictures.
//!
//! The renderer asks for a picture by its `src` and is answered at once from the device's folder of pictures, or with nothing
//! while the picture is fetched (with the person's token) and kept there.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// The most the folder of pictures keeps.
pub const MOST_BYTES: u64 = 120 * 1024 * 1024;

/// Where the picture of `src` is kept: a name made from the address.
#[must_use]
pub fn file_for(dir: &Path, src: &str) -> PathBuf {
    // FNV-1a: the same address is the same file on every run.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in src.bytes() {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    dir.join(format!("{hash:016x}"))
}

/// The pictures in the folder, oldest first, taken out until what is left is no more than `most` bytes.
pub fn trim(dir: &Path, most: u64) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<_> = read
        .flatten()
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            Some((meta.modified().ok()?, meta.len(), e.path()))
        })
        .collect();
    let mut total: u64 = files.iter().map(|f| f.1).sum();
    files.sort();
    for (_, size, path) in files {
        if total <= most {
            break;
        }
        if std::fs::remove_file(path).is_ok() {
            total -= size;
        }
    }
}

/// Keeps a picture the service sent.
pub fn keep(dir: &Path, src: &str, bytes: &[u8]) {
    let _ = std::fs::create_dir_all(dir);
    let file = file_for(dir, src);
    let temp = file.with_extension("tmp");
    if std::fs::write(&temp, bytes).is_ok() {
        let _ = std::fs::rename(temp, file);
    }
}

/// The pictures asked for and not yet here.
#[derive(Debug, Default)]
pub struct Wanted {
    asked: HashSet<String>,
}

impl Wanted {
    /// Whether `src` was not asked for already (and now is).
    pub fn first(&mut self, src: &str) -> bool {
        self.asked.insert(src.to_string())
    }

    /// The picture came (or could not): it may be asked for again.
    pub fn done(&mut self, src: &str) {
        self.asked.remove(src);
    }
}
