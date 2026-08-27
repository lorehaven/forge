//! `reap_failed` - removes `Failed`-status instances (which vLLM pods never
//! recover from, per the module docs) and leaves everything else alone.
//! `MockVllmEngine` never produces a `Failed` instance itself, so this uses a
//! small dedicated test engine that can be told exactly what to report.

use async_trait::async_trait;
use chrono::Utc;
use std::sync::{Arc, Mutex};
use switchboard_service::routers::vllm::engine::VllmEngine;
use switchboard_service::routers::vllm::reaper::reap_failed;
use switchboard_service::routers::vllm::types::{LaunchRequest, VllmInstance};

fn instance(id: &str, status: &str) -> VllmInstance {
    VllmInstance {
        id: id.to_string(),
        namespace: "default".to_string(),
        model: "m".to_string(),
        host: "0.0.0.0".to_string(),
        port: 8000,
        quantization: None,
        dtype: None,
        limit_mm_per_prompt: None,
        max_model_len: None,
        gpu_memory_utilization: None,
        enable_prefix_caching: false,
        enable_tool_calling: false,
        task: None,
        device: None,
        started_at: Utc::now(),
        status: status.to_string(),
        log_path: None,
        last_error: None,
    }
}

struct RecordingEngine {
    instances: Vec<VllmInstance>,
    stopped: Arc<Mutex<Vec<String>>>,
    fail_list: bool,
    fail_stop_for: Option<String>,
}

#[async_trait]
impl VllmEngine for RecordingEngine {
    async fn list_instances(&self) -> Result<Vec<VllmInstance>, String> {
        if self.fail_list {
            return Err("backend unreachable".to_string());
        }
        Ok(self.instances.clone())
    }

    async fn launch_instance(&self, _req: LaunchRequest) -> Result<VllmInstance, String> {
        unimplemented!("not exercised by reaper tests")
    }

    async fn stop_instance(&self, id: String) -> Result<(), String> {
        if self.fail_stop_for.as_deref() == Some(id.as_str()) {
            return Err(format!("could not stop {id}"));
        }
        self.stopped.lock().unwrap().push(id);
        Ok(())
    }
}

#[tokio::test]
async fn reap_failed_stops_only_failed_instances() {
    let stopped = Arc::new(Mutex::new(Vec::new()));
    let engine: Arc<dyn VllmEngine> = Arc::new(RecordingEngine {
        instances: vec![
            instance("running-1", "running"),
            instance("failed-1", "failed"),
            instance("failed-2", "Failed"),
            instance("starting-1", "starting"),
        ],
        stopped: stopped.clone(),
        fail_list: false,
        fail_stop_for: None,
    });

    reap_failed(&engine).await;

    // Only the exact "Failed" (capital F) status - set at launch time by the
    // real engines - is reaped; "failed" (lowercase, a UI-facing display
    // value) is left alone.
    let stopped = stopped.lock().unwrap().clone();
    assert_eq!(stopped, vec!["failed-2".to_string()]);
}

#[tokio::test]
async fn reap_failed_does_nothing_when_listing_fails() {
    let stopped = Arc::new(Mutex::new(Vec::new()));
    let engine: Arc<dyn VllmEngine> = Arc::new(RecordingEngine {
        instances: vec![],
        stopped: stopped.clone(),
        fail_list: true,
        fail_stop_for: None,
    });

    reap_failed(&engine).await;

    assert!(stopped.lock().unwrap().is_empty());
}

#[tokio::test]
async fn reap_failed_continues_past_a_stop_failure_to_reap_the_rest() {
    let stopped = Arc::new(Mutex::new(Vec::new()));
    let engine: Arc<dyn VllmEngine> = Arc::new(RecordingEngine {
        instances: vec![instance("Failed", "Failed"), instance("Failed-2", "Failed")],
        stopped: stopped.clone(),
        fail_list: false,
        fail_stop_for: Some("Failed".to_string()),
    });

    reap_failed(&engine).await;

    assert_eq!(
        stopped.lock().unwrap().clone(),
        vec!["Failed-2".to_string()]
    );
}

#[tokio::test]
async fn reap_failed_is_a_no_op_when_nothing_is_failed() {
    let stopped = Arc::new(Mutex::new(Vec::new()));
    let engine: Arc<dyn VllmEngine> = Arc::new(RecordingEngine {
        instances: vec![instance("a", "running"), instance("b", "starting")],
        stopped: stopped.clone(),
        fail_list: false,
        fail_stop_for: None,
    });

    reap_failed(&engine).await;

    assert!(stopped.lock().unwrap().is_empty());
}
