use welder::backend::{get, init};
use welder::config::CONFIG;

/// `BACKEND` is a module-level `OnceLock` inside the `welder` library,
/// shared by every test in this binary (they all link the same lib, so it's
/// one process-wide static) - it can only ever be set once, successfully,
/// for the whole process. So `init`/`get` get exactly one test, exercising
/// the full "first call succeeds, second call reports already-initialized,
/// `get` then works" sequence in one place, rather than one test per
/// behavior racing each other over the same static.
///
/// This crate's `cli/welder/.welder/config.toml` sets `backend.kind =
/// "vllm"` for the `cargo test` cwd, so the config-reading branches for
/// `"ollama"`/`"switchboard"` (which need `ollama_url`/`switchboard_url` to
/// be `Some`) aren't reachable here without changing that fixture -
/// `Config::load`'s parsing itself is covered separately in
/// `config_tests`/`app_config_tests`.
#[test]
fn init_succeeds_once_then_get_returns_the_vllm_backend() {
    // `CONFIG` is a `LazyLock` that locks in whatever `cwd` was at its first
    // touch from anywhere in this process, forever - hold the crate-wide cwd
    // lock while forcing it here so a `config`/`app_config` test can't be
    // mid-way through pointing the cwd at an empty tempdir when that happens
    // (see `support::cwd_lock`'s doc comment).
    let _guard = crate::support::cwd_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    assert_eq!(CONFIG.backend.kind, "vllm");

    init().expect("first init with the vllm backend should succeed");
    let second = init().unwrap_err();
    assert!(second.to_string().contains("more than once"));

    assert!(get().is_running());
}
