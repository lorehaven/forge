use pulley::service::task_command;
use std::path::Path;

#[quench_test::test]
fn the_scheduled_task_runs_the_daemon_without_a_window() {
    assert_eq!(
        task_command(Path::new(r"C:\Program Files\pulley\pulley.exe")),
        r#"conhost.exe --headless "C:\Program Files\pulley\pulley.exe" daemon"#
    );
}
