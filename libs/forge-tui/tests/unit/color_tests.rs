use forge_tui::{ColorLevel, Rgb};

#[quench_test::test]
fn hex_splits_the_channels() {
    assert_eq!(Rgb::hex(0x17b26a), Rgb(0x17, 0xb2, 0x6a));
}

#[quench_test::test]
fn over_composites_like_css_rgba() {
    // rgba(240, 68, 56, 0.14) over #141517
    let got = Rgb::hex(0xf04438).over(Rgb::hex(0x141517), 14);
    assert_eq!(got, Rgb(51, 28, 28));
    assert_eq!(
        Rgb(10, 20, 30).over(Rgb(200, 200, 200), 100),
        Rgb(10, 20, 30)
    );
    assert_eq!(
        Rgb(10, 20, 30).over(Rgb(200, 200, 200), 0),
        Rgb(200, 200, 200)
    );
}

#[quench_test::test]
fn the_256_palette_has_the_cube_and_a_grey_ramp() {
    assert_eq!(Rgb(0, 0, 0).to_256(), 16);
    assert_eq!(Rgb(255, 255, 255).to_256(), 231);
    assert_eq!(Rgb(255, 0, 0).to_256(), 196);
    // Near-greys use the ramp (232..=255), not the cube.
    let grey = Rgb::hex(0x141517).to_256();
    assert!((232..=255).contains(&grey), "{grey}");
}

#[quench_test::test]
fn no_color_wins_over_everything() {
    assert_eq!(
        ColorLevel::from_vars("1", "xterm", "truecolor"),
        ColorLevel::None
    );
    assert_eq!(
        ColorLevel::from_vars("", "dumb", "truecolor"),
        ColorLevel::None
    );
}

#[quench_test::test]
fn colorterm_selects_truecolor() {
    assert_eq!(
        ColorLevel::from_vars("", "xterm", "truecolor"),
        ColorLevel::TrueColor
    );
    assert_eq!(
        ColorLevel::from_vars("", "xterm", "24bit"),
        ColorLevel::TrueColor
    );
    assert_eq!(
        ColorLevel::from_vars("", "xterm-256color", ""),
        ColorLevel::Ansi256
    );
}
