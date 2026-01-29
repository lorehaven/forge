use ferrous::llm::decoding::StopCondition;

#[test]
fn test_stop_condition_simple() {
    let mut sc = StopCondition::new(vec!["STOP".to_string()]);
    assert!(!sc.should_stop("HELLO").0);
    assert!(sc.should_stop("STOP").0);
}

#[test]
fn test_stop_condition_incremental() {
    let mut sc = StopCondition::new(vec!["\n\n".to_string()]);
    assert!(!sc.should_stop("first line").0);
    assert!(!sc.should_stop("\n").0);
    assert!(sc.should_stop("\n").0);
}

#[test]
fn test_stop_condition_multi_word() {
    let mut sc = StopCondition::new(vec!["END OF STREAM".to_string()]);
    assert!(!sc.should_stop("END OF ").0);
    assert!(sc.should_stop("STREAM").0);
}
