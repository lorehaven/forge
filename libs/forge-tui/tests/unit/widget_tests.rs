use forge_tui::widgets::{self, ButtonKind};
use forge_tui::{ColorLevel, Theme, Tone};

const fn theme() -> Theme {
    Theme::dark()
}

#[quench_test::test]
fn badge_is_a_dot_and_text_on_the_soft_background() {
    let badge = widgets::badge(&theme(), Tone::Success, "Up to date");
    assert_eq!(badge.plain(), " ● Up to date ");
    let soft = theme().soft(Tone::Success);
    let out = badge.render(ColorLevel::TrueColor);
    assert!(
        out.contains(&format!(
            "38;2;{};{};{}",
            soft.text.0, soft.text.1, soft.text.2
        )),
        "{out}"
    );
    assert!(
        out.contains(&format!("48;2;{};{};{}", soft.bg.0, soft.bg.1, soft.bg.2)),
        "{out}"
    );
}

#[quench_test::test]
fn hint_pairs_a_key_chip_with_its_label() {
    assert_eq!(widgets::hint(&theme(), "q", "Quit").plain(), " q  Quit");
}

#[quench_test::test]
fn primary_button_is_solid_brand_and_disabled_has_no_fill() {
    let t = theme();
    let primary = widgets::button(&t, ButtonKind::Primary, "Install").render(ColorLevel::TrueColor);
    let brand = t.brand_solid;
    assert!(
        primary.contains(&format!("48;2;{};{};{}", brand.0, brand.1, brand.2)),
        "{primary}"
    );
    let disabled = widgets::button(&t, ButtonKind::Disabled, "-").render(ColorLevel::TrueColor);
    assert!(!disabled.contains("48;"), "{disabled}");
}

#[quench_test::test]
fn alert_fills_the_width_and_leads_with_its_icon() {
    let alert = widgets::alert(&theme(), Tone::Warning, "update available", 40);
    assert_eq!(alert.width(), 40);
    assert!(
        alert.plain().starts_with("▌ ! update available"),
        "{}",
        alert.plain()
    );
}

#[quench_test::test]
fn card_draws_a_rounded_box_of_the_requested_width() {
    let t = theme();
    let header = forge_tui::Line::styled("Tools", forge_tui::Style::new());
    let body = vec![forge_tui::Line::styled("anvil", forge_tui::Style::new())];
    let lines = widgets::card(&t, Some(header), body, 20);
    let plain: Vec<String> = lines.iter().map(forge_tui::Line::plain).collect();
    assert_eq!(plain.len(), 5);
    assert!(plain.iter().all(|l| l.chars().count() == 20), "{plain:?}");
    assert!(plain[0].starts_with('╭') && plain[0].ends_with('╮'));
    assert!(plain[2].starts_with('├') && plain[2].ends_with('┤'));
    assert!(plain[3].starts_with("│anvil") && plain[3].ends_with('│'));
    assert!(plain[4].starts_with('╰') && plain[4].ends_with('╯'));
}

#[quench_test::test]
fn card_without_a_header_has_no_divider() {
    let lines = widgets::card(&theme(), None, vec![forge_tui::Line::new()], 10);
    assert_eq!(lines.len(), 3);
}
