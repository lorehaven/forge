//! Unit tests for `observability/metrics.rs`.

use sage_service::observability::metrics::*;

#[test]
fn test_tool_metrics_recording() {
    let mut metrics = ToolMetrics::new();
    metrics.record_execution(100, true, false);
    metrics.record_execution(200, true, false);
    metrics.record_execution(150, false, false);

    assert_eq!(metrics.calls, 3);
    assert_eq!(metrics.successes, 2);
    assert_eq!(metrics.failures, 1);
    assert_eq!(metrics.total_duration_ms, 450);
    assert_eq!(metrics.min_duration_ms, 100);
    assert_eq!(metrics.max_duration_ms, 200);
    assert!((metrics.success_rate() - 2.0 / 3.0).abs() < 0.001);
    assert!((metrics.avg_duration_ms() - 150.0).abs() < 0.001);
}

#[test]
fn test_metrics_collector() {
    let collector = MetricsCollector::new();

    collector.record_tool_execution("web_assistant", "web_search", 500, true, false);
    collector.record_tool_execution("web_assistant", "web_search", 600, true, false);
    collector.record_tool_execution("web_assistant", "calculator", 50, true, false);

    let profile_metrics = collector.get_profile_metrics("web_assistant").unwrap();
    assert_eq!(profile_metrics.total_calls, 3);
    assert_eq!(profile_metrics.total_successes, 3);
    assert_eq!(profile_metrics.tools.len(), 2);
}
