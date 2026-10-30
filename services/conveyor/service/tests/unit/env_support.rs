//! One lock for the tests that read or write the process-global variables the kubernetes executor
//! takes its settings from (`KUBECONFIG`, `CONVEYOR_K8S_*`). They share one binary, so a lock per file
//! would not stop a test in one file reading what a test in another has just set.

use std::sync::{Mutex, MutexGuard, OnceLock};

pub fn kubernetes_env() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
