pub mod admin;
pub mod crates;
pub mod docker;
pub mod files;
pub mod ui;

static CRATES_STORAGE_ROOT: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| envmnt::get_or("CRATES_STORAGE_PATH", "./storage/crates"));

static DOCKER_STORAGE_ROOT: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| envmnt::get_or("STORAGE_PATH", "./storage/docker"));

struct FeatureFlags {
    docker: bool,
    crates: bool,
    files: bool,
}

static FEATURE_FLAGS: std::sync::LazyLock<FeatureFlags> =
    std::sync::LazyLock::new(|| FeatureFlags {
        docker: feature_enabled("FEATURE_DOCKER_ENABLED", false),
        crates: feature_enabled("FEATURE_CRATES_ENABLED", false),
        files: feature_enabled("FEATURE_FILES_ENABLED", false),
    });

fn feature_enabled(name: &str, default: bool) -> bool {
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
