//! Coverage for `repl.rs`'s dispatcher and small helpers that don't touch a
//! real cluster. `current_kube_context`/`announce_target`'s "no pinned
//! context" branch do shell out to `kubectl config current-context`, but
//! that's a read-only local kubeconfig read, never a cluster call - safe to
//! run for real, unlike `kubectl apply`/`diff`/`delete` (left untested here
//! deliberately: this machine has a real `kubectl` on PATH, and those would
//! either hang or mutate whatever context is configured).

use crate::env_support::cwd_lock;
use riveter::env::ENV_VAR;
use riveter::render::RenderedManifest;
use riveter::repl::{
    announce_target, current_kube_context, error, handle_repl_command, print_block, prompt,
    repl_help,
};

fn empty_rendered(kube_context: Option<String>) -> RenderedManifest {
    RenderedManifest {
        path: "manifests/x.yaml".to_string(),
        resource_count: 0,
        selected: Vec::new(),
        kube_context,
        skipped_out_of_scope: Vec::new(),
        namespace: None,
        creates_namespace: false,
    }
}

/// Runs `body` in a fresh temp cwd with no `overlays/` and `$RIVETER_ENV`
/// cleared, so `current_env()` deterministically errors - same locking
/// discipline as `env_tests`' helper, since cwd and env vars are
/// process-global state every test in this binary shares.
fn in_temp_cwd_with_no_env<T>(body: impl FnOnce() -> T) -> T {
    let _guard = cwd_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("overlays")).unwrap();
    let original = std::env::current_dir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();
    envmnt::remove(ENV_VAR);

    let result = body();

    envmnt::remove(ENV_VAR);
    std::env::set_current_dir(original).unwrap();
    result
}

#[test]
fn prompt_falls_back_to_unset_without_a_configured_environment() {
    in_temp_cwd_with_no_env(|| {
        assert!(prompt().contains("unset"), "{}", prompt());
    });
}

#[test]
fn print_block_and_error_do_not_panic() {
    print_block("some help text\nacross lines");
    error("something went wrong");
}

#[test]
fn repl_help_overview_and_unknown_topic_do_not_panic() {
    repl_help(None);
    repl_help(Some("targets"));
    repl_help(Some("target"));
    repl_help(Some("apply"));
    repl_help(Some("not-a-real-topic"));
}

#[test]
fn handle_repl_command_on_empty_input_continues() {
    assert!(!handle_repl_command("").unwrap());
    assert!(!handle_repl_command("   ").unwrap());
}

#[test]
fn handle_repl_command_help_continues() {
    assert!(!handle_repl_command("help").unwrap());
    assert!(!handle_repl_command("h apply").unwrap());
}

#[test]
fn handle_repl_command_exit_variants_signal_exit() {
    assert!(handle_repl_command("exit").unwrap());
    assert!(handle_repl_command("quit").unwrap());
    assert!(handle_repl_command("q").unwrap());
}

#[test]
fn handle_repl_command_unknown_command_continues() {
    assert!(!handle_repl_command("frobnicate").unwrap());
}

#[test]
fn handle_repl_command_env_needing_commands_error_without_a_configured_environment() {
    in_temp_cwd_with_no_env(|| {
        for cmd in ["list", "render", "diff", "prune", "apply", "delete"] {
            let err = handle_repl_command(cmd).unwrap_err();
            assert!(!err.to_string().is_empty(), "{cmd} should error");
        }
    });
}

#[test]
fn handle_repl_command_env_list_and_show_work_without_a_current_environment() {
    in_temp_cwd_with_no_env(|| {
        // `env list` and `env show` don't need a *set* current env the way
        // `list`/`render`/etc do - `env list` just reads the (empty)
        // `overlays/` directory, and `env show` goes through `current_env`
        // like the others and errors the same way.
        assert!(!handle_repl_command("env list").unwrap());
        let err = handle_repl_command("env show").unwrap_err();
        assert!(!err.to_string().is_empty());
    });
}

#[test]
fn announce_target_with_a_pinned_context_does_not_shell_out() {
    // `kube_context: Some(..)` short-circuits before `current_kube_context`
    // is ever called - nothing here touches a process.
    announce_target("prod", &empty_rendered(Some("prod-cluster".to_string())));
}

#[test]
fn announce_target_without_a_pinned_context_reads_the_local_kubeconfig() {
    // No `kube_context` set falls through to `current_kube_context()`,
    // which only runs `kubectl config current-context` - a local,
    // read-only kubeconfig lookup, not a cluster call.
    announce_target("prod", &empty_rendered(None));
}

/// Runs `body` in a fresh temp cwd with `overlays/<env>/overlay.yaml` set to
/// a small, real overlay and `$RIVETER_ENV` pointed at it - `list`/`render`
/// never touch kubectl (only `apply`/`diff`/`delete`/`prune` do), so both are
/// safe to exercise for real here.
fn in_temp_cwd_with_env<T>(env: &str, body: impl FnOnce() -> T) -> T {
    let _guard = cwd_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let dir = tempfile::tempdir().unwrap();
    let overlay_dir = dir.path().join("overlays").join(env);
    std::fs::create_dir_all(&overlay_dir).unwrap();
    std::fs::write(
        overlay_dir.join("overlay.yaml"),
        "resources:\n  - kind: deployment\n    name: api\n    image: nginx\n",
    )
    .unwrap();

    let original = std::env::current_dir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();
    envmnt::set(ENV_VAR, env);

    let result = body();

    envmnt::remove(ENV_VAR);
    std::env::set_current_dir(original).unwrap();
    result
}

#[test]
fn handle_repl_command_list_prints_the_configured_environment_s_resources() {
    in_temp_cwd_with_env("golden", || {
        assert!(!handle_repl_command("list").unwrap());
        assert!(!handle_repl_command("ls").unwrap());
    });
}

#[test]
fn handle_repl_command_render_writes_manifests_for_the_configured_environment() {
    in_temp_cwd_with_env("golden", || {
        assert!(!handle_repl_command("render").unwrap());
        assert!(std::path::Path::new("manifests/golden-manifests.mutable.yaml").exists());
    });
}

#[test]
fn current_kube_context_does_not_panic() {
    // Whatever this machine's kubeconfig says (probably nothing) - just
    // confirm the read-only lookup completes without panicking.
    let _ = current_kube_context();
}
