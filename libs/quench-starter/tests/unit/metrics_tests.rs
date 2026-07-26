//! Unit tests for `metrics.rs`.

use quench_starter::metrics::*;
use std::time::Duration;

#[test]
fn test_metrics_success() {
    let metrics = RequestMetrics::new();
    metrics.record_success(100);
    metrics.record_success(200);

    let snap = metrics.snapshot();
    assert_eq!(snap.total_requests, 2);
    assert_eq!(snap.success_count, 2);
    assert_eq!(snap.error_count, 0);
    assert_eq!(snap.avg_latency_ms, 150); // (100 + 200) / 2
}

#[test]
fn test_metrics_error() {
    let metrics = RequestMetrics::new();
    metrics.record_success(100);
    metrics.record_error("Connection timeout");

    let snap = metrics.snapshot();
    assert_eq!(snap.total_requests, 2);
    assert_eq!(snap.success_count, 1);
    assert_eq!(snap.error_count, 1);
    assert!(snap.last_error.as_ref().unwrap().contains("timeout"));
}

#[test]
fn test_timed_block() {
    let block = TimedBlock::new();
    std::thread::sleep(Duration::from_millis(10));
    assert!(block.elapsed_ms() >= 10);
}
