//! Unit tests for `tools/code_executor.rs`.

use sage_service::tools::code_executor::*;

#[test]
fn test_python_safety_validation() {
    assert!(validate_code_safety("print('hello')", "python").is_ok());
    assert!(validate_code_safety("os.system('ls')", "python").is_err());
    assert!(validate_code_safety("exec('code')", "python").is_err());
}

#[test]
fn test_javascript_safety_validation() {
    assert!(validate_code_safety("console.log('hello')", "javascript").is_ok());
    assert!(validate_code_safety("eval('code')", "javascript").is_err());
    assert!(validate_code_safety("fetch('url')", "javascript").is_err());
}

#[test]
fn test_code_length_validation() {
    let long_code = "a".repeat(5001);
    assert!(validate_code_safety(&long_code, "python").is_err());
}
