use sha2::{Digest, Sha256};
use std::fmt::Write;
use std::path::Path;
use tokio::io::AsyncReadExt;

pub fn sha256_hex(data: impl AsRef<[u8]>) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);

    hex_of(hasher.finalize())
}

/// Hex SHA-256 of a file's contents, read in 64 KiB chunks so hashing a
/// multi-gigabyte blob costs one small buffer rather than the whole file in
/// memory. Used when finalising a Docker blob upload.
pub async fn sha256_file(path: impl AsRef<Path>) -> std::io::Result<String> {
    let mut file = tokio::fs::File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];

    loop {
        let read = file.read(&mut buf).await?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
    }

    Ok(hex_of(hasher.finalize()))
}

fn hex_of(digest: impl AsRef<[u8]>) -> String {
    let digest = digest.as_ref();
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(&mut out, "{byte:02x}").unwrap();
    }
    out
}
