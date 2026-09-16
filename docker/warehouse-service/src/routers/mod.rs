pub mod admin;
pub mod artifacts;
pub mod crates;
pub mod docker;
pub mod files;
pub mod ui;

/// Links every route module in - an unreferenced `.rlib` module is dead-code-eliminated
/// before `inventory` ever sees its routes.
pub fn register_routes() {
    admin::register_routes();
    artifacts::register_routes();
    crates::register_routes();
    docker::register_routes();
    files::register_routes();
    ui::register_routes();
}

/// Read fresh every call, not `LazyLock`-cached - a process-global cache would make this untestable.
pub fn crates_storage_root() -> String {
    envmnt::get_or("CRATES_STORAGE_PATH", "./storage/crates")
}

pub fn docker_storage_root() -> String {
    envmnt::get_or("STORAGE_PATH", "./storage/docker")
}

/// Root of the artifact store. `APK_STORAGE_PATH` is an honored fallback for the old var name.
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
        // `FEATURE_APK_ENABLED` is a fallback - it now gates the whole artifact store.
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
