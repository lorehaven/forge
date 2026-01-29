use ferrous::core::plan::{ExecutionPlan, StepStatus};

#[test]
fn test_execution_plan_new() {
    let descriptions = vec!["Step 1".to_string(), "Step 2".to_string()];
    let plan = ExecutionPlan::new(descriptions);
    
    assert_eq!(plan.steps.len(), 2);
    assert_eq!(plan.steps[0].id, 1);
    assert_eq!(plan.steps[0].description, "Step 1");
    assert!(matches!(plan.steps[0].status, StepStatus::Pending));
}

#[test]
fn test_plan_status_transitions() {
    let mut plan = ExecutionPlan::new(vec!["Step 1".to_string()]);
    
    plan.mark_running(1);
    assert!(matches!(plan.steps[0].status, StepStatus::Running));
    
    plan.mark_done(1);
    assert!(matches!(plan.steps[0].status, StepStatus::Done));
    
    plan.mark_failed(1, "Error".to_string());
    assert!(matches!(plan.steps[0].status, StepStatus::Failed(ref s) if s == "Error"));
}

#[test]
fn test_plan_display() {
    let mut plan = ExecutionPlan::new(vec!["Step 1".to_string()]);
    let output = format!("{}", plan);
    assert!(output.contains("1. Step 1"));
    
    plan.mark_done(1);
    let output = format!("{}", plan);
    assert!(output.contains("[v]"));
}
