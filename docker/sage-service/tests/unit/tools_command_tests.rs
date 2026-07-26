//! Unit tests for `tools/command.rs`.

use sage_service::tools::command::*;

#[test]
fn test_safe_commands() {
    assert!(is_safe_command("cat /etc/hostname"));
    assert!(is_safe_command("ls -la"));
    assert!(is_safe_command("grep pattern file.txt"));
    assert!(is_safe_command("pwd"));
    assert!(is_safe_command("date"));
}

#[test]
fn test_dangerous_commands() {
    assert!(!is_safe_command("rm -rf /"));
    assert!(!is_safe_command("sudo cat file"));
    assert!(!is_safe_command("cat file > /tmp/out"));
    assert!(!is_safe_command("kill -9 1234"));
    assert!(!is_safe_command("dd if=/dev/zero of=/dev/sda"));
}
