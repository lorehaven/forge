//! Unit tests for `observability/cost_tracking.rs`.

use sage_service::observability::cost_tracking::*;

#[test]
fn test_request_cost_creation() {
    let cost = RequestCost::new(
        Some("user-1".to_string()),
        Some("conv-1".to_string()),
        "web_assistant".to_string(),
    );
    assert_eq!(cost.user_id, Some("user-1".to_string()));
    assert_eq!(cost.total_tokens_used, 0);
}

#[test]
fn test_add_tool_cost() {
    let mut cost = RequestCost::new(
        Some("user-1".to_string()),
        Some("conv-1".to_string()),
        "web_assistant".to_string(),
    );

    cost.add_tool_cost(ToolCost {
        tool_name: "web_search".to_string(),
        tokens_used: 100,
        api_calls: 1,
        duration_ms: 500,
    });

    assert_eq!(cost.total_tokens_used, 100);
    assert_eq!(cost.total_api_calls, 1);
    assert_eq!(cost.total_duration_ms, 500);
}

#[test]
fn test_cost_tracker() {
    let tracker = CostTracker::new();
    let mut cost = RequestCost::new(
        Some("user-1".to_string()),
        None,
        "web_assistant".to_string(),
    );
    cost.add_tool_cost(ToolCost {
        tool_name: "web_search".to_string(),
        tokens_used: 100,
        api_calls: 1,
        duration_ms: 500,
    });

    tracker.record_request_cost(cost);

    let user_costs = tracker.get_user_costs("user-1").unwrap();
    assert_eq!(user_costs.request_count, 1);
    assert_eq!(user_costs.total_tokens_used, 100);
}

#[test]
fn test_estimate_cost() {
    let mut cost = RequestCost::new(None, None, "web_assistant".to_string());
    cost.total_tokens_used = 1_000_000; // 1M tokens
    cost.total_api_calls = 100;

    let estimated = cost.estimate_cost();
    // 1M tokens * $0.000002 + 100 calls * $0.001 = $2 + $0.1 = $2.1
    assert!((estimated - 2.1).abs() < 0.01);
}
