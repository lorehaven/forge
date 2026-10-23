pub mod admin;
pub mod artifacts;
pub mod crates;
pub mod docker;
pub mod files;
pub mod rivets;
pub mod ui;

/// Links every route module in - an unreferenced `.rlib` module is dead-code-eliminated
/// before `inventory` ever sees its routes.
pub fn register_routes() {
    admin::register_routes();
    artifacts::register_routes();
    crates::register_routes();
    docker::register_routes();
    files::register_routes();
    rivets::register_routes();
    ui::register_routes();
}

/// Read fresh every call, not `LazyLock`-cached - a process-global cache would make this untestable.
#[must_use]
pub fn crates_storage_root() -> String {
    quench_core::env::get_or("CRATES_STORAGE_PATH", "./storage/crates")
}

#[must_use]
pub fn docker_storage_root() -> String {
    quench_core::env::get_or("STORAGE_PATH", "./storage/docker")
}

/// Root of the artifact store. `APK_STORAGE_PATH` is an honored fallback for the old var name.
#[must_use]
pub fn artifact_storage_root() -> String {
    for key in ["ARTIFACT_STORAGE_PATH", "APK_STORAGE_PATH"] {
        if std::env::var_os(key).is_some() {
            return quench_core::env::get_or(key, "");
        }
    }
    "./storage/artifacts".to_string()
}

/// Root of the rivet package store.
#[must_use]
pub fn rivet_storage_root() -> String {
    quench_core::env::get_or("RIVET_STORAGE_PATH", "./storage/rivets")
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Feature {
    Docker,
    Crates,
    Files,
    Artifacts,
}

/// The features switched on, read once at first use.
static FEATURE_FLAGS: std::sync::LazyLock<std::collections::BTreeSet<Feature>> =
    std::sync::LazyLock::new(|| {
        [
            (
                Feature::Docker,
                feature_enabled("FEATURE_DOCKER_ENABLED", false),
            ),
            (
                Feature::Crates,
                feature_enabled("FEATURE_CRATES_ENABLED", false),
            ),
            (
                Feature::Files,
                feature_enabled("FEATURE_FILES_ENABLED", false),
            ),
            // `FEATURE_APK_ENABLED` is a fallback - it now gates the whole artifact store.
            (
                Feature::Artifacts,
                feature_enabled("FEATURE_ARTIFACTS_ENABLED", false)
                    || feature_enabled("FEATURE_APK_ENABLED", false),
            ),
        ]
        .into_iter()
        .filter_map(|(feature, on)| on.then_some(feature))
        .collect()
    });

#[must_use]
pub fn feature_enabled(name: &str, default: bool) -> bool {
    match quench_core::env::get_or(name, if default { "true" } else { "false" })
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "1" | "true" | "yes" | "on" => true,
        "0" | "false" | "no" | "off" => false,
        _ => default,
    }
}

#[must_use]
pub fn docker_enabled() -> bool {
    FEATURE_FLAGS.contains(&Feature::Docker)
}

#[must_use]
pub fn crates_enabled() -> bool {
    FEATURE_FLAGS.contains(&Feature::Crates)
}

#[must_use]
pub fn files_enabled() -> bool {
    FEATURE_FLAGS.contains(&Feature::Files)
}

#[must_use]
pub fn artifacts_enabled() -> bool {
    FEATURE_FLAGS.contains(&Feature::Artifacts)
}

/// Read fresh every call, unlike the other flags, so a test can turn it on for one case.
#[must_use]
pub fn rivets_enabled() -> bool {
    feature_enabled("FEATURE_RIVETS_ENABLED", false)
}
