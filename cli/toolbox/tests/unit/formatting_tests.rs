use forge_toolbox::{
    CrateStatus, action_label, display_installed, display_latest, display_updatable, fit_cell,
    make_border,
};
use semver::Version;

fn status(
    installed: Option<&str>,
    latest: Option<&str>,
    installable: bool,
    updatable: bool,
) -> CrateStatus {
    CrateStatus {
        package: "anvil",
        binary: "anvil",
        installed_version: installed.map(|v| Version::parse(v).unwrap()),
        latest_version: latest.map(|v| Version::parse(v).unwrap()),
        installable,
        updatable,
        error: None,
    }
}

#[test]
fn fit_cell_pads_short_values() {
    assert_eq!(fit_cell("ab", 5), "ab   ");
}

#[test]
fn fit_cell_zero_width_is_empty() {
    assert_eq!(fit_cell("anything", 0), "");
}

#[test]
fn fit_cell_exact_width_is_unchanged() {
    assert_eq!(fit_cell("abcde", 5), "abcde");
}

#[test]
fn fit_cell_truncates_with_tilde_marker() {
    assert_eq!(fit_cell("abcdef", 5), "abcd~");
}

#[test]
fn fit_cell_width_one_truncation_has_no_room_for_marker() {
    // width > 1 is required to append '~', so width == 1 just takes 1 char.
    assert_eq!(fit_cell("abcdef", 1), "a");
}

#[test]
fn fit_cell_handles_multibyte_chars_by_count_not_bytes() {
    // "héllo" is 5 chars but more than 5 bytes; width is measured in chars.
    assert_eq!(fit_cell("héllo", 5), "héllo");
    assert_eq!(fit_cell("héllo!", 5), "héll~");
}

#[test]
fn display_installed_shows_version_when_present() {
    let s = status(Some("1.2.3"), Some("1.2.3"), true, false);
    assert_eq!(display_installed(&s), "1.2.3");
}

#[test]
fn display_installed_shows_can_install_when_missing_but_installable() {
    let s = status(None, Some("1.0.0"), true, false);
    assert_eq!(display_installed(&s), "no (can install)");
}

#[test]
fn display_installed_shows_plain_no_when_not_installable() {
    let s = status(None, None, false, false);
    assert_eq!(display_installed(&s), "no");
}

#[test]
fn display_latest_shows_na_when_unknown() {
    let s = status(Some("1.0.0"), None, false, false);
    assert_eq!(display_latest(&s), "n/a");
}

#[test]
fn display_latest_shows_version_when_known() {
    let s = status(None, Some("2.0.0"), true, false);
    assert_eq!(display_latest(&s), "2.0.0");
}

#[test]
fn display_updatable_yes_no() {
    assert_eq!(
        display_updatable(&status(Some("1.0.0"), Some("2.0.0"), true, true)),
        "yes"
    );
    assert_eq!(
        display_updatable(&status(Some("1.0.0"), Some("1.0.0"), true, false)),
        "no"
    );
}

#[test]
fn action_label_install_when_missing_and_installable() {
    let s = status(None, Some("1.0.0"), true, false);
    assert_eq!(action_label(&s), "install");
}

#[test]
fn action_label_update_when_updatable() {
    let s = status(Some("1.0.0"), Some("2.0.0"), true, true);
    assert_eq!(action_label(&s), "update");
}

#[test]
fn action_label_dash_when_up_to_date() {
    let s = status(Some("1.0.0"), Some("1.0.0"), true, false);
    assert_eq!(action_label(&s), "-");
}

#[test]
fn action_label_dash_when_not_installable_and_not_installed() {
    let s = status(None, None, false, false);
    assert_eq!(action_label(&s), "-");
}

#[test]
fn make_border_uses_given_corner_and_junction_chars() {
    let widths = forge_toolbox::content_widths(&[]);
    let border = make_border('┌', '┬', '┐', 2, &widths);
    assert!(border.starts_with('┌'));
    assert!(border.ends_with('┐'));
    // marker + 6 columns = 7 segments, joined by 6 '┬' characters.
    assert_eq!(border.matches('┬').count(), 6);
}
