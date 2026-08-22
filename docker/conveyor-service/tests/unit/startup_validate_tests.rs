//! `startup::report_toolchain` - the startup toolchain check. Never refuses
//! to start, only logs, so these just confirm it doesn't panic under either
//! executor kind rather than asserting on log output.

use conveyor_service::config::{ConveyorConfig, ExecutorKind};
use conveyor_service::startup::report_toolchain;

fn config_with_executor(executor: ExecutorKind) -> ConveyorConfig {
    ConveyorConfig {
        executor,
        ..Default::default()
    }
}

#[test]
fn does_not_panic_when_the_executor_is_kubernetes() {
    // The early-return branch: the local toolchain is irrelevant under this
    // executor, so this must not even attempt a `which` lookup that could
    // itself misbehave.
    report_toolchain(&config_with_executor(ExecutorKind::Kubernetes));
}

#[test]
fn does_not_panic_when_the_executor_is_native() {
    // Exercises the `which::which` lookups for every optional tool, whether
    // or not they're actually on this machine's `PATH`.
    report_toolchain(&config_with_executor(ExecutorKind::Native));
}
