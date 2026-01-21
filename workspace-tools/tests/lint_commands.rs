use workspace_tools::commands::lint;

#[test]
#[ignore]
fn lint_clippy() {
    let _ = lint::lint(true, true, true);
}

#[test]
#[ignore]
fn lint_clippy_no_warnings() {
    let _ = lint::lint(true, true, false);
}

#[test]
#[ignore]
fn format_check() {
    let _ = lint::format(true);
}

#[test]
#[ignore]
fn format_apply() {
    let _ = lint::format(false);
}
