use forge_toolbox::{
    CrateStatus, action_label, action_text, display_installed, display_latest, fit_cell,
    message_tone, note_tone, status_kind,
};
use forge_tui::Tone;
use quench_core::version::Version;

fn status(
    installed: Option<&str>,
    latest: Option<&str>,
    installable: bool,
    updatable: bool,
) -> CrateStatus {
    CrateStatus {
        package: "anvil".to_string(),
        binary: "anvil".to_string(),
        installed_version: installed.map(|v| Version::parse(v).unwrap()),
        latest_version: latest.map(|v| Version::parse(v).unwrap()),
        installable,
        updatable,
        error: None,
    }
}

#[quench_test::test]
fn fit_cell_pads_short_values() {
    assert_eq!(fit_cell("ab", 5), "ab   ");
}

#[quench_test::test]
fn fit_cell_zero_width_is_empty() {
    assert_eq!(fit_cell("anything", 0), "");
}

#[quench_test::test]
fn fit_cell_exact_width_is_unchanged() {
    assert_eq!(fit_cell("abcde", 5), "abcde");
}

#[quench_test::test]
fn fit_cell_truncates_with_tilde_marker() {
    assert_eq!(fit_cell("abcdef", 5), "abcd~");
}

#[quench_test::test]
fn fit_cell_width_one_truncation_has_no_room_for_marker() {
    // width > 1 is required to append '~', so width == 1 just takes 1 char.
    assert_eq!(fit_cell("abcdef", 1), "a");
}

#[quench_test::test]
fn fit_cell_handles_multibyte_chars_by_count_not_bytes() {
    // "héllo" is 5 chars but more than 5 bytes; width is measured in chars.
    assert_eq!(fit_cell("héllo", 5), "héllo");
    assert_eq!(fit_cell("héllo!", 5), "héll~");
}

#[quench_test::test]
fn display_installed_shows_version_when_present() {
    let s = status(Some("1.2.3"), Some("1.2.3"), true, false);
    assert_eq!(display_installed(&s), "1.2.3");
}

#[quench_test::test]
fn display_installed_shows_a_dash_when_missing() {
    let s = status(None, Some("1.0.0"), true, false);
    assert_eq!(display_installed(&s), "—");
}

#[quench_test::test]
fn display_installed_shows_a_dash_when_not_installable() {
    let s = status(None, None, false, false);
    assert_eq!(display_installed(&s), "—");
}

#[quench_test::test]
fn display_latest_shows_a_dash_when_unknown() {
    let s = status(Some("1.0.0"), None, false, false);
    assert_eq!(display_latest(&s), "—");
}

#[quench_test::test]
fn display_latest_shows_version_when_known() {
    let s = status(None, Some("2.0.0"), true, false);
    assert_eq!(display_latest(&s), "2.0.0");
}

#[quench_test::test]
fn action_label_install_when_missing_and_installable() {
    let s = status(None, Some("1.0.0"), true, false);
    assert_eq!(action_label(&s), "install");
}

#[quench_test::test]
fn action_label_update_when_updatable() {
    let s = status(Some("1.0.0"), Some("2.0.0"), true, true);
    assert_eq!(action_label(&s), "update");
}

#[quench_test::test]
fn action_label_dash_when_up_to_date() {
    let s = status(Some("1.0.0"), Some("1.0.0"), true, false);
    assert_eq!(action_label(&s), "-");
}

#[quench_test::test]
fn action_label_dash_when_not_installable_and_not_installed() {
    let s = status(None, None, false, false);
    assert_eq!(action_label(&s), "-");
}

#[quench_test::test]
fn status_kind_maps_every_state_to_a_badge() {
    let kind = |s: &CrateStatus| status_kind(s);
    assert_eq!(
        kind(&status(Some("1.0.0"), Some("1.0.0"), true, false)),
        (Tone::Success, "Up to date")
    );
    assert_eq!(
        kind(&status(Some("1.0.0"), Some("2.0.0"), true, true)),
        (Tone::Warning, "Update available")
    );
    assert_eq!(
        kind(&status(None, Some("1.0.0"), true, false)),
        (Tone::Gray, "Not installed")
    );
    assert_eq!(
        kind(&status(None, None, false, false)),
        (Tone::Gray, "Unavailable")
    );
    assert_eq!(
        kind(&status(Some("1.0.0"), None, false, false)),
        (Tone::Gray, "Not in registry")
    );
    let mut failed = status(Some("1.0.0"), None, false, false);
    failed.error = Some("boom".to_string());
    assert_eq!(kind(&failed), (Tone::Error, "Registry error"));
}

#[quench_test::test]
fn action_text_names_the_button() {
    assert_eq!(
        action_text(&status(None, Some("1.0.0"), true, false)),
        "Install"
    );
    assert_eq!(
        action_text(&status(Some("1.0.0"), Some("2.0.0"), true, true)),
        "Update"
    );
    assert_eq!(
        action_text(&status(Some("1.0.0"), Some("1.0.0"), true, false)),
        "—"
    );
}

#[quench_test::test]
fn note_tone_follows_the_self_update_banner() {
    assert_eq!(
        note_tone(
            "note: forge-toolbox update available: 0.1.0 -> 0.2.0. run forge-toolbox self-update"
        ),
        Tone::Warning
    );
    assert_eq!(
        note_tone("note: forge-toolbox is up to date"),
        Tone::Success
    );
    assert_eq!(
        note_tone("note: forge-toolbox is not installed (latest 0.2.0)"),
        Tone::Info
    );
    assert_eq!(note_tone("note: could not determine latest"), Tone::Gray);
}

#[quench_test::test]
fn message_tone_marks_failures_and_successes() {
    assert_eq!(message_tone("action failed: x"), Tone::Error);
    assert_eq!(
        message_tone("anvil is not installable from registry ennor"),
        Tone::Error
    );
    assert_eq!(message_tone("anvil updated"), Tone::Success);
    assert_eq!(message_tone("anvil installed"), Tone::Success);
    assert_eq!(message_tone("Ready"), Tone::Gray);
}
