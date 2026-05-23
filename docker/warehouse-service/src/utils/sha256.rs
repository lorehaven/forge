use sha2::{Digest, Sha256};
use std::fmt::Write;

pub fn sha256_hex(data: impl AsRef<[u8]>) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);

    let digest = hasher.finalize();
    let mut out = String::with_capacity(digest.len() * 2);

    for byte in digest {
        write!(&mut out, "{:02x}", byte).unwrap();
    }

    out
}
