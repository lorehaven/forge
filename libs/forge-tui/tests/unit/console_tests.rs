use forge_tui::console::{
    Tone, accent_with, banner_with, chip_with, muted_with, rule_with, section_with, status_with,
    strong_with,
};
use forge_tui::{ColorLevel, Theme};

const DARK: Theme = Theme::dark();

#[quench_test::test]
fn plain_status_is_the_label_then_the_message() {
    let out = status_with(&DARK, ColorLevel::None, Tone::Success, "build", "done");
    assert_eq!(out, " build  done");
}

#[quench_test::test]
fn a_status_label_is_a_badge_on_the_soft_background() {
    let soft = DARK.soft(forge_tui::Tone::Error);
    let out = status_with(&DARK, ColorLevel::TrueColor, Tone::Error, "fail", "boom");
    assert!(
        out.contains(&format!("48;2;{};{};{}", soft.bg.0, soft.bg.1, soft.bg.2)),
        "{out:?}"
    );
    assert!(
        out.ends_with("\x1b[0m boom"),
        "plain message text is not coloured: {out:?}"
    );
}

#[quench_test::test]
fn an_empty_label_prints_just_the_message() {
    assert_eq!(
        status_with(&DARK, ColorLevel::TrueColor, Tone::Info, "", "hi"),
        "hi"
    );
}

#[quench_test::test]
fn chip_pads_inside_the_badge() {
    assert_eq!(
        chip_with(&DARK, ColorLevel::None, Tone::Info, "ab", 5),
        " ab    "
    );
}

#[quench_test::test]
fn banner_is_a_rounded_card_of_equal_rows() {
    let rows = banner_with(
        &DARK,
        ColorLevel::None,
        "Anvil CLI",
        "workspace build and release",
    );
    assert_eq!(rows.len(), 4);
    assert!(rows[0].starts_with('╭') && rows[3].ends_with('╯'));
    assert!(rows[1].contains("Anvil CLI") && rows[2].contains("workspace build"));
    assert!(rows.iter().all(|r| r.chars().count() == 49), "{rows:?}");
}

#[quench_test::test]
fn section_has_an_accent_bar_title_and_detail() {
    assert_eq!(
        section_with(&DARK, ColorLevel::None, "WORKFLOW", "flow.toml"),
        "▌ WORKFLOW  flow.toml"
    );
    assert_eq!(
        section_with(&DARK, ColorLevel::None, "Answer", ""),
        "▌ Answer"
    );
}

#[quench_test::test]
fn helpers_are_plain_without_colour() {
    assert_eq!(rule_with(&DARK, ColorLevel::None, 3), "───");
    assert_eq!(muted_with(&DARK, ColorLevel::None, "x"), "x");
    assert_eq!(strong_with(ColorLevel::None, "x"), "x");
    assert_eq!(accent_with(&DARK, ColorLevel::None, "x"), "x");
    assert!(strong_with(ColorLevel::TrueColor, "x").starts_with("\x1b[1m"));
}
