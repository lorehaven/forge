//! What the registry says and what the machine has.

use quench_serde::Deserialize;

/// One build in the registry, as every read returns it (warehouse's `ArtifactView`).
///
/// Order by [`version_code`](Self::version_code), never by `version_name`: the name is free text from the
/// build, the code is the monotonic integer that decides "newer". The `sha256` is bare lowercase hex.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Artifact {
    /// The program: an application id on Android (`dev.lorehaven.mathom`), a name elsewhere.
    pub program: String,
    /// `android`, `linux`, `windows` or `macos`.
    pub platform: String,
    /// `x86_64`, `aarch64` or `universal`; absent for Android's one APK.
    #[serde(default)]
    pub arch: Option<String>,
    /// `apk`, `deb`, `tar.gz`, ...: what the publisher said the file is.
    pub format: String,
    pub version_code: i64,
    pub version_name: String,
    /// The name the download is served as.
    pub filename: String,
    pub size_bytes: i64,
    pub sha256: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub min_sdk_version: Option<i32>,
    #[serde(default)]
    pub target_sdk_version: Option<i32>,
    #[serde(default)]
    pub permissions: Vec<String>,
    pub uploaded_by: String,
    #[serde(default)]
    pub yanked: bool,
    /// RFC 3339.
    pub created_at: String,
}

impl Artifact {
    /// What a person sees: the label, else the program.
    #[must_use]
    pub fn title(&self) -> &str {
        self.label
            .as_deref()
            .filter(|l| !l.is_empty())
            .unwrap_or(&self.program)
    }
}

/// A program that is on this machine.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Installed {
    pub program: String,
    pub version_code: i64,
    #[serde(default)]
    pub version_name: String,
}

/// `41758291` as `39.8 MiB`: what the detail screen shows for a size.
#[must_use]
pub fn format_bytes(bytes: i64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    if bytes < 1024 {
        return format!("{} B", bytes.max(0));
    }
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}
