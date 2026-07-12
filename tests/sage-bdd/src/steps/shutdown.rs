use crate::steps::common::SageWorld;
use std::sync::{Mutex, OnceLock};
use tokio::time::{Duration, sleep};

/// PID of the sage-service process under test, recorded by the harness after
/// launch so the shutdown scenario can signal it directly.
static SAGE_PID: OnceLock<u32> = OnceLock::new();

/// Instance ids the mock switchboard received DELETE (graceful stop) requests
/// for. Populated by the mock request handler in `main`.
static DELETED_INSTANCES: OnceLock<Mutex<Vec<String>>> = OnceLock::new();

fn deleted_instances() -> &'static Mutex<Vec<String>> {
    DELETED_INSTANCES.get_or_init(|| Mutex::new(Vec::new()))
}

/// Record the sage-service PID so the shutdown scenario can send it a signal.
pub fn set_sage_pid(pid: u32) {
    let _ = SAGE_PID.set(pid);
}

/// Called by the mock switchboard when it receives a stop request.
pub fn record_deleted_instance(id: String) {
    deleted_instances().lock().unwrap().push(id);
}

#[cucumber::given("the sage service was started with model teardown enabled")]
async fn started_with_teardown(_world: &mut SageWorld) -> Result<(), String> {
    // The harness launches sage with SAGE_STOP_MODELS_ON_SHUTDOWN=true and
    // records its PID; here we just assert that wiring is in place.
    if SAGE_PID.get().is_none() {
        return Err("sage-service PID was not recorded by the harness".to_string());
    }
    Ok(())
}

#[cucumber::when("the sage service receives a termination signal")]
async fn send_termination_signal(_world: &mut SageWorld) -> Result<(), String> {
    let pid = *SAGE_PID
        .get()
        .ok_or_else(|| "sage-service PID was not recorded by the harness".to_string())?;

    let status = tokio::process::Command::new("kill")
        .arg("-TERM")
        .arg(pid.to_string())
        .status()
        .await
        .map_err(|e| format!("failed to send SIGTERM to sage-service: {e}"))?;

    if !status.success() {
        return Err(format!("kill -TERM {pid} exited with {status}"));
    }
    Ok(())
}

#[cucumber::then("switchboard should be asked to stop the default model instance")]
async fn switchboard_stop_requested(_world: &mut SageWorld) -> Result<(), String> {
    // Graceful teardown runs after actix finishes its shutdown, so the DELETE
    // arrives shortly after the signal — poll for it rather than assuming timing.
    for _ in 0..40 {
        if deleted_instances()
            .lock()
            .unwrap()
            .iter()
            .any(|id| id == "mock-1782724283792")
        {
            return Ok(());
        }
        sleep(Duration::from_millis(250)).await;
    }
    Err("switchboard never received a stop request for the default model instance".to_string())
}
