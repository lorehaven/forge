pub mod admin;
pub mod artifacts;
pub mod crates;
pub mod docker;
pub mod files;
pub mod ui;

/// Read fresh on every call rather than cached in a `LazyLock` - a
/// process-global cache initialized from whichever env var value happened to
/// be set the first time anything touched it would make this untestable
/// (every test in the binary would be stuck with whatever the first test set,
/// or the real default). The cost is a `HashMap` lookup per storage-path
/// resolution, negligible next to the filesystem I/O around it.
pub fn crates_storage_root() -> String {
    envmnt::get_or("CRATES_STORAGE_PATH", "./storage/crates")
}

pub fn docker_storage_root() -> String {
    envmnt::get_or("STORAGE_PATH", "./storage/docker")
}

/// Root of the multi-platform artifact store. `ARTIFACT_STORAGE_PATH` is the
/// current name; `APK_STORAGE_PATH` is honoured as a fallback so a deployment
/// that set the old var keeps working (its APK subtree is relocated once on
/// boot - see `crate::relocate_legacy_apk_storage`).
pub fn artifact_storage_root() -> String {
    for key in ["ARTIFACT_STORAGE_PATH", "APK_STORAGE_PATH"] {
        if envmnt::exists(key) {
            return envmnt::get_or(key, "");
        }
    }
    "./storage/artifacts".to_string()
}

struct FeatureFlags {
    docker: bool,
    crates: bool,
    files: bool,
    artifacts: bool,
}

static FEATURE_FLAGS: std::sync::LazyLock<FeatureFlags> =
    std::sync::LazyLock::new(|| FeatureFlags {
        docker: feature_enabled("FEATURE_DOCKER_ENABLED", false),
        crates: feature_enabled("FEATURE_CRATES_ENABLED", false),
        files: feature_enabled("FEATURE_FILES_ENABLED", false),
        // `FEATURE_APK_ENABLED` kept as a fallback: the store it gated is now
        // the whole artifact store, Android included.
        artifacts: feature_enabled("FEATURE_ARTIFACTS_ENABLED", false)
            || feature_enabled("FEATURE_APK_ENABLED", false),
    });

pub fn feature_enabled(name: &str, default: bool) -> bool {
    match envmnt::get_or(name, if default { "true" } else { "false" })
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "1" | "true" | "yes" | "on" => true,
        "0" | "false" | "no" | "off" => false,
        _ => default,
    }
}

pub fn docker_enabled() -> bool {
    FEATURE_FLAGS.docker
}

pub fn crates_enabled() -> bool {
    FEATURE_FLAGS.crates
}

pub fn files_enabled() -> bool {
    FEATURE_FLAGS.files
}

pub fn artifacts_enabled() -> bool {
    FEATURE_FLAGS.artifacts
}
