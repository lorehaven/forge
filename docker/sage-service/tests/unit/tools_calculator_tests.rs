//! Unit tests for `tools/calculator.rs`.

use sage_service::tools::calculator::*;

#[test]
fn test_basic_operations() {
    assert_eq!(eval_expression("2 + 2").unwrap(), 4.0);
    assert_eq!(eval_expression("10 - 3").unwrap(), 7.0);
    assert_eq!(eval_expression("4 * 5").unwrap(), 20.0);
    assert_eq!(eval_expression("20 / 4").unwrap(), 5.0);
}

#[test]
fn test_complex_expressions() {
    assert_eq!(eval_expression("2 + 3 * 4").unwrap(), 14.0);
    assert_eq!(eval_expression("(2 + 3) * 4").unwrap(), 20.0);
    assert_eq!(eval_expression("2 ^ 3").unwrap(), 8.0);
}

#[test]
fn test_functions() {
    assert!((eval_expression("sqrt(16)").unwrap() - 4.0).abs() < 0.001);
    assert!((eval_expression("abs(-5)").unwrap() - 5.0).abs() < 0.001);
}
