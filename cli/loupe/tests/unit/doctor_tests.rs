use loupe::doctor::{Probe, report};
use quench_cli::graphics::Rgb;
use quench_cli::graphics::detect::{Capabilities, Environment};

fn kitty_terminal() -> Probe {
    Probe {
        environment: Environment {
            term: Some("xterm-kitty".to_string()),
            graphics: None,
            multiplexer: false,
            terminal: true,
        },
        capabilities: Capabilities {
            kitty: true,
            sixel: false,
            cell_size_px: Some((9, 18)),
            text_area_px: Some((720, 432)),
            foreground: Some(Rgb::new(0xee, 0xee, 0xee)),
            background: Some(Rgb::new(0x10, 0x10, 0x10)),
        },
        font: "Noto Sans Math, bundled (quench-text)".to_string(),
        typeset: Ok((213, 56)),
    }
}

fn doctor(probe: &Probe) -> (i32, String) {
    let mut out = Vec::new();
    let status = report(probe, &mut out).unwrap();
    (status, String::from_utf8(out).unwrap())
}

#[quench_test::test]
fn kitty_with_the_bundled_font_typesets() {
    let (status, out) = doctor(&kitty_terminal());
    assert_eq!(status, 0, "{out}");
    assert_eq!(
        out,
        "loupe doctor\n\
         \x20 math font   Noto Sans Math, bundled (quench-text): typesets (a sample formula drew 213×56 px)\n\
         \x20 graphics    kitty: the terminal answered the kitty graphics query\n\
         \x20 cell size   9×18 px\n\
         \x20 colours     foreground #eeeeee, background #101010\n\
         \x20 formulas    typeset as images (kitty, drawn in #eeeeee)\n"
    );
}

#[quench_test::test]
fn sixel_is_reported_from_device_attributes() {
    let mut probe = kitty_terminal();
    probe.capabilities.kitty = false;
    probe.capabilities.sixel = true;
    let (status, out) = doctor(&probe);
    assert_eq!(status, 0);
    assert!(
        out.contains("sixel: the terminal's device attributes include sixel (4)"),
        "{out}"
    );
}

#[quench_test::test]
fn no_terminal_falls_back_to_text() {
    let probe = Probe {
        environment: Environment::default(),
        capabilities: Capabilities::NONE,
        font: "Noto Sans Math, bundled (quench-text)".to_string(),
        typeset: Ok((1, 1)),
    };
    let (status, out) = doctor(&probe);
    assert_eq!(status, 1);
    assert!(
        out.contains("graphics    none: stdin or stdout is not a terminal"),
        "{out}"
    );
    assert!(
        out.contains("cell size   not measured (assumed 10×20 px when drawing)"),
        "{out}"
    );
    assert!(
        out.contains("foreground not measured, background not measured"),
        "{out}"
    );
    assert!(
        out.contains("formulas    shown as text: the terminal shows no images"),
        "{out}"
    );
}

#[quench_test::test]
fn the_reasons_for_no_graphics() {
    let mut probe = kitty_terminal();
    probe.capabilities = Capabilities::NONE;
    let (_, out) = doctor(&probe);
    assert!(
        out.contains("answered neither the kitty graphics query nor with sixel"),
        "{out}"
    );
    probe.environment.multiplexer = true;
    let (_, out) = doctor(&probe);
    assert!(out.contains("inside tmux or screen"), "{out}");
    probe.environment.multiplexer = false;
    probe.environment.term = Some("dumb".to_string());
    let (_, out) = doctor(&probe);
    assert!(out.contains("TERM is dumb"), "{out}");
    probe.environment.graphics = Some("none".to_string());
    let (status, out) = doctor(&probe);
    assert_eq!(status, 1);
    assert!(out.contains("none: QUENCH_GRAPHICS=none"), "{out}");
}

#[quench_test::test]
fn a_forced_protocol_is_named() {
    let mut probe = kitty_terminal();
    probe.environment.graphics = Some("sixel".to_string());
    probe.capabilities.kitty = false;
    probe.capabilities.sixel = true;
    let (status, out) = doctor(&probe);
    assert_eq!(status, 0);
    assert!(
        out.contains("sixel: forced by QUENCH_GRAPHICS=sixel"),
        "{out}"
    );
}

#[quench_test::test]
fn a_chosen_font_that_cannot_be_used_falls_back_to_text() {
    let mut probe = kitty_terminal();
    probe.font = "/fonts/Latin.otf (--math-font)".to_string();
    probe.typeset =
        Err("/fonts/Latin.otf is a CFF font: CFF outlines are not read yet".to_string());
    let (status, out) = doctor(&probe);
    assert_eq!(status, 1);
    assert!(
        out.contains(
            "  math font   /fonts/Latin.otf (--math-font): not used: /fonts/Latin.otf is a CFF font: CFF \
             outlines are not read yet\n"
        ),
        "{out}"
    );
    assert!(
        out.contains("shown as text: the math font cannot be used"),
        "{out}"
    );
}

#[quench_test::test]
fn a_chosen_font_that_works_is_named_with_its_source() {
    let mut probe = kitty_terminal();
    probe.font = "/fonts/Math.ttf (the config file's math_font)".to_string();
    let (status, out) = doctor(&probe);
    assert_eq!(status, 0);
    assert!(
        out.contains("  math font   /fonts/Math.ttf (the config file's math_font): typesets"),
        "{out}"
    );
}

#[quench_test::test]
fn the_bundled_font_typesets_here() {
    let (w, h) = loupe::graphics::check_typeset(&quench_tex::typeset::Fonts::bundled()).unwrap();
    assert!(w > 50 && h > 20, "{w}×{h}");
}
