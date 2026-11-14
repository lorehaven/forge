use forge_toolbox::{App, CrateStatus, frame, render, render_with};
use forge_tui::{ColorLevel, Theme};
use quench_core::version::Version;

fn v(s: &str) -> Version {
    Version::parse(s).unwrap()
}

fn app(statuses: Vec<CrateStatus>, selected: usize) -> App {
    App {
        selected,
        statuses,
        toolbox_note: "note: forge-toolbox is up to date".to_string(),
        message: "Ready".to_string(),
    }
}

fn rendered(statuses: Vec<CrateStatus>, selected: usize, transient: Option<&str>) -> String {
    render(&app(statuses, selected), transient, 120).join("\n")
}

#[quench_test::test]
fn header_and_controls_are_always_present() {
    let out = rendered(Vec::new(), 0, None);
    assert!(out.contains("Forge Toolbox"));
    assert!(out.contains("Registry"));
    assert!(out.contains("Navigate"));
    assert!(out.contains("Quit"));
}

#[quench_test::test]
fn package_and_binary_names_appear_in_their_row() {
    let statuses = vec![CrateStatus {
        package: "anvil".to_string(),
        binary: "anvil".to_string(),
        installed_version: Some(v("1.0.0")),
        latest_version: Some(v("1.0.0")),
        installable: true,
        updatable: false,
        error: None,
    }];
    let out = rendered(statuses, 0, None);
    assert!(out.contains("anvil"));
    assert!(out.contains("1.0.0"));
}

#[quench_test::test]
fn an_updatable_crate_shows_the_update_action() {
    let statuses = vec![CrateStatus {
        package: "riveter".to_string(),
        binary: "riveter".to_string(),
        installed_version: Some(v("1.0.0")),
        latest_version: Some(v("2.0.0")),
        installable: true,
        updatable: true,
        error: None,
    }];
    let out = rendered(statuses, 0, None);
    assert!(out.contains("Update"));
    assert!(out.contains("Update available"));
}

#[quench_test::test]
fn an_uninstalled_installable_crate_shows_the_install_action() {
    let statuses = vec![CrateStatus {
        package: "welder".to_string(),
        binary: "welder".to_string(),
        installed_version: None,
        latest_version: Some(v("1.0.0")),
        installable: true,
        updatable: false,
        error: None,
    }];
    let out = rendered(statuses, 0, None);
    assert!(out.contains("Install"));
    assert!(out.contains("Not installed"));
}

#[quench_test::test]
fn a_crate_with_an_error_shows_a_warning_line() {
    let statuses = vec![CrateStatus {
        package: "pulley".to_string(),
        binary: "pulley".to_string(),
        installed_version: None,
        latest_version: None,
        installable: false,
        updatable: false,
        error: Some("registry lookup failed".to_string()),
    }];
    let out = rendered(statuses, 0, None);
    assert!(out.contains("Registry error"));
    assert!(out.contains("registry lookup failed"));
}

#[quench_test::test]
fn the_selected_row_is_marked() {
    let statuses = vec![
        CrateStatus {
            package: "a".to_string(),
            binary: "a".to_string(),
            installed_version: None,
            latest_version: None,
            installable: false,
            updatable: false,
            error: None,
        },
        CrateStatus {
            package: "b".to_string(),
            binary: "b".to_string(),
            installed_version: None,
            latest_version: None,
            installable: false,
            updatable: false,
            error: None,
        },
    ];
    let out = render(&app(statuses, 1), None, 120);
    let plain = plain_rows(statuses_for_marker(), 1);
    assert!(
        plain
            .iter()
            .any(|line| line.contains("│▌") && line.contains(" b "))
    );
    assert_eq!(plain.iter().filter(|line| line.contains("│▌")).count(), 1);
    assert!(!out.is_empty());
}

#[quench_test::test]
fn a_transient_status_overrides_the_app_message() {
    let out = rendered(Vec::new(), 0, Some("⠋ running anvil (installing)"));
    assert!(out.contains("running anvil (installing)"));
    assert!(!out.contains("Ready"));
}

#[quench_test::test]
fn with_no_transient_status_the_app_message_is_shown() {
    let out = rendered(Vec::new(), 0, None);
    assert!(out.contains("Ready"));
}

#[quench_test::test]
fn a_very_narrow_terminal_still_renders_every_line() {
    let statuses = vec![CrateStatus {
        package: "warehouse-cli".to_string(),
        binary: "warehouse".to_string(),
        installed_version: Some(v("1.0.0")),
        latest_version: Some(v("1.2.0")),
        installable: true,
        updatable: true,
        error: None,
    }];
    let lines = render(&app(statuses, 0), None, 20);
    assert!(!lines.is_empty());
}

fn statuses_for_marker() -> Vec<CrateStatus> {
    ["a", "b"]
        .into_iter()
        .map(|name| CrateStatus {
            package: name.to_string(),
            binary: name.to_string(),
            installed_version: None,
            latest_version: None,
            installable: false,
            updatable: false,
            error: None,
        })
        .collect()
}

fn plain_rows(statuses: Vec<CrateStatus>, selected: usize) -> Vec<String> {
    render_with(
        &app(statuses, selected),
        None,
        100,
        0,
        &Theme::dark(),
        ColorLevel::None,
    )
}

fn sample() -> Vec<CrateStatus> {
    vec![
        CrateStatus {
            package: "anvil".to_string(),
            binary: "anvil".to_string(),
            installed_version: Some(v("1.0.0")),
            latest_version: Some(v("1.0.0")),
            installable: true,
            updatable: false,
            error: None,
        },
        CrateStatus {
            package: "riveter".to_string(),
            binary: "riveter".to_string(),
            installed_version: Some(v("1.0.0")),
            latest_version: Some(v("2.0.0")),
            installable: true,
            updatable: true,
            error: None,
        },
    ]
}

#[quench_test::test]
fn plain_output_has_no_escape_codes() {
    let rows = plain_rows(sample(), 0);
    assert!(rows.iter().all(|row| !row.contains('\x1b')));
}

#[quench_test::test]
fn the_card_header_counts_packages_and_updates() {
    let out = plain_rows(sample(), 0).join("\n");
    assert!(out.contains("2 packages"), "{out}");
    assert!(out.contains("1 update"), "{out}");
}

#[quench_test::test]
fn a_truecolor_screen_is_painted_to_the_full_size_on_the_page_background() {
    let theme = Theme::dark();
    let rows = render_with(
        &app(sample(), 0),
        None,
        90,
        40,
        &theme,
        ColorLevel::TrueColor,
    );
    assert_eq!(rows.len(), 40);
    let bg = format!("48;2;{};{};{}", theme.bg.0, theme.bg.1, theme.bg.2);
    // Every row starts on the page background, down to the empty rows at the bottom.
    assert!(rows.iter().all(|row| row.contains(&bg)));
    // Visible width of each row is the terminal width.
    for row in &rows {
        let mut width = 0;
        let mut in_escape = false;
        for ch in row.chars() {
            match (in_escape, ch) {
                (_, '\x1b') => in_escape = true,
                (true, 'm') => in_escape = false,
                (true, _) => {}
                (false, _) => width += 1,
            }
        }
        assert_eq!(width, 90, "{row:?}");
    }
}

#[quench_test::test]
fn the_selected_row_uses_the_hover_surface() {
    let theme = Theme::dark();
    let rows = render_with(
        &app(sample(), 1),
        None,
        100,
        0,
        &theme,
        ColorLevel::TrueColor,
    );
    let hover = format!(
        "48;2;{};{};{}",
        theme.surface_hover.0, theme.surface_hover.1, theme.surface_hover.2
    );
    let with_hover: Vec<&String> = rows.iter().filter(|row| row.contains(&hover)).collect();
    assert_eq!(with_hover.len(), 1);
    assert!(with_hover[0].contains("riveter"));
}

#[quench_test::test]
fn a_light_theme_changes_the_page_background() {
    let light = Theme::from_vars("light", "", "");
    let rows = render_with(
        &app(sample(), 0),
        None,
        90,
        10,
        &light,
        ColorLevel::TrueColor,
    );
    let bg = format!("48;2;{};{};{}", light.bg.0, light.bg.1, light.bg.2);
    assert!(rows[0].contains(&bg));
}

#[quench_test::test]
fn narrow_terminals_hide_columns_but_keep_the_row_width() {
    let rows = render_with(
        &app(sample(), 0),
        None,
        50,
        0,
        &Theme::dark(),
        ColorLevel::None,
    );
    assert!(rows.iter().all(|row| row.chars().count() <= 50), "{rows:?}");
    assert!(rows.join("\n").contains("riveter"));
}

fn many(count: usize) -> Vec<CrateStatus> {
    (0..count)
        .map(|n| CrateStatus {
            package: format!("tool-{n:02}"),
            binary: format!("tool-{n:02}"),
            installed_version: Some(v("1.0.0")),
            latest_version: Some(v("1.0.0")),
            installable: true,
            updatable: false,
            error: None,
        })
        .collect()
}

#[quench_test::test]
fn a_short_terminal_scrolls_the_table_and_keeps_the_message_visible() {
    let theme = Theme::dark();
    let rows = render_with(&app(many(20), 0), None, 100, 24, &theme, ColorLevel::None);
    assert_eq!(rows.len(), 24);
    assert!(rows[23].contains("Ready"), "{:?}", rows[23]);
    let text = rows.join("\n");
    assert!(text.contains("tool-00"));
    assert!(!text.contains("tool-19"));
    assert!(text.contains("of 20"), "{text}");
}

#[quench_test::test]
fn the_scrolled_table_follows_the_selection() {
    let theme = Theme::dark();
    let rows = render_with(&app(many(20), 19), None, 100, 24, &theme, ColorLevel::None);
    let text = rows.join("\n");
    assert!(text.contains("tool-19"));
    assert!(!text.contains("tool-00"));
    assert!(rows[23].contains("Ready"));
}

#[quench_test::test]
fn a_tall_terminal_shows_every_tool_without_a_range() {
    let rows = render_with(
        &app(many(5), 0),
        None,
        100,
        40,
        &Theme::dark(),
        ColorLevel::None,
    );
    let text = rows.join("\n");
    assert!(text.contains("5 packages"));
    assert!(!text.contains(" of 5"));
}

#[quench_test::test]
fn a_frame_overwrites_in_place_and_never_clears_the_screen() {
    let painted = frame(&["one".to_string(), "two".to_string()], "");
    assert!(!painted.contains("\x1b[2J"), "{painted:?}");
    assert!(painted.starts_with("\x1b[H"));
    assert!(painted.contains("one\x1b[K\x1b[0m\r\ntwo\x1b[K"));
    assert!(!painted.contains("2026"));
}

#[quench_test::test]
fn a_frame_erases_each_row_in_the_page_background() {
    let erase = Theme::dark().bg.background_escape(ColorLevel::TrueColor);
    assert_eq!(erase, "\x1b[48;2;13;14;16m");
    let painted = frame(&["one".to_string()], &erase);
    assert!(
        painted.contains("one\x1b[48;2;13;14;16m\x1b[K"),
        "{painted:?}"
    );
    assert_eq!(Theme::dark().bg.background_escape(ColorLevel::None), "");
}
