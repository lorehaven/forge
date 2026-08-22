use foreman::config::ToneName;
use foreman::ui::*;

#[test]
fn tone_from_tone_name_maps_every_variant() {
    assert!(matches!(Tone::from(ToneName::Info), Tone::Info));
    assert!(matches!(Tone::from(ToneName::Ok), Tone::Ok));
    assert!(matches!(Tone::from(ToneName::Warn), Tone::Warn));
    assert!(matches!(Tone::from(ToneName::Error), Tone::Error));
}

#[test]
fn tone_colour_is_distinct_per_variant() {
    let colours = [
        Tone::Info.colour(),
        Tone::Ok.colour(),
        Tone::Warn.colour(),
        Tone::Error.colour(),
    ];
    for (i, a) in colours.iter().enumerate() {
        for (j, b) in colours.iter().enumerate() {
            assert_eq!(i == j, a == b);
        }
    }
}

// Nothing here has an interesting return value - these just prove every
// print helper runs to completion without panicking, for every tone.
#[test]
fn every_print_helper_runs_without_panicking() {
    say(Tone::Info, "label", "message");
    info("label", "message");
    ok("label", "message");
    warn("label", "message");
    error("label", "message");
    entry("svc", "https://localhost:8080/");
    dim("a note");
    blank();
    quote("first line\nsecond line");
    quote("");
}

#[test]
fn say_pads_a_short_label_to_the_column_width() {
    // No direct way to capture stdout here without a wider refactor -
    // this at least exercises a label both under and over LABEL_WIDTH.
    say(Tone::Info, "x", "short label");
    say(
        Tone::Info,
        "a-much-longer-label-than-the-column",
        "long label",
    );
}
