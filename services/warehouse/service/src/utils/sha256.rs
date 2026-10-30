use std::fmt::Write;
use std::path::Path;
use tokio::io::AsyncReadExt;

pub fn sha256_hex(data: impl AsRef<[u8]>) -> String {
    let mut hasher = quench_crypto::hash::Hasher::new(quench_crypto::hash::Algorithm::Sha256);
    hasher.update(data);

    hex_of(hasher.finish())
}

/// Hex SHA-256 of a file, read in 64 KiB chunks so a multi-GB blob costs one small buffer.
pub async fn sha256_file(path: impl AsRef<Path>) -> std::io::Result<String> {
    let mut file = tokio::fs::File::open(path).await?;
    let mut hasher = quench_crypto::hash::Hasher::new(quench_crypto::hash::Algorithm::Sha256);
    let mut buf = vec![0u8; 64 * 1024];

    loop {
        let read = file.read(&mut buf).await?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
    }

    Ok(hex_of(hasher.finish()))
}

fn hex_of(digest: impl AsRef<[u8]>) -> String {
    let digest = digest.as_ref();
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(&mut out, "{byte:02x}").unwrap();
    }
    out
}
