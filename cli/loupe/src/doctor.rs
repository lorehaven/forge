//! `loupe doctor`: what works here, and why not. The probe is passed in, so the report can be tested with a
//! made-up terminal.

use crate::graphics::{DEFAULT_CELL, Graphics};
use quench_cli::graphics::Rgb;
use quench_cli::graphics::detect::{Capabilities, Environment, Forced, Protocol, plan};
use std::io::{self, Write};

/// What `doctor` found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Probe {
    pub environment: Environment,
    pub capabilities: Capabilities,
    /// The math font in use and where it comes from (`Noto Sans Math, bundled (quench-text)`, or a path and
    /// whether the command line or the config file chose it).
    pub font: String,
    /// A sample formula typeset with that font: the picture's size in pixels, or why the font cannot be used.
    pub typeset: Result<(u32, u32), String>,
}

/// Why the terminal shows no images, or how it was found that it does.
#[must_use]
pub fn graphics_reason(environment: &Environment, capabilities: &Capabilities) -> String {
    let plan = plan(environment);
    let forced = match plan.forced {
        Some(Forced::Kitty) => Some("kitty"),
        Some(Forced::Sixel) => Some("sixel"),
        Some(Forced::None) => return "none: QUENCH_GRAPHICS=none".to_string(),
        None => None,
    };
    match (capabilities.protocol(), forced) {
        (Some(_), Some(name)) => format!("{name}: forced by QUENCH_GRAPHICS={name}"),
        (Some(Protocol::Kitty), None) => {
            "kitty: the terminal answered the kitty graphics query".to_string()
        }
        (Some(Protocol::Sixel), None) => {
            "sixel: the terminal's device attributes include sixel (4)".to_string()
        }
        (None, _) => {
            let why = if !environment.terminal {
                "stdin or stdout is not a terminal, so nothing was asked".to_string()
            } else if environment.term.as_deref().is_none_or(str::is_empty) {
                "TERM is not set, so nothing was asked".to_string()
            } else if environment.term.as_deref() == Some("dumb") {
                "TERM is dumb, so nothing was asked".to_string()
            } else if environment.multiplexer {
                "inside tmux or screen nothing is asked (QUENCH_GRAPHICS=kitty or sixel forces a protocol)".to_string()
            } else {
                "the terminal answered neither the kitty graphics query nor with sixel in its device attributes"
                    .to_string()
            };
            format!("none: {why}")
        }
    }
}

fn hex(c: Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
}

/// Writes the report; returns the exit status: 0 when formulas are typeset as images, 1 when they fall back to
/// text.
///
/// # Errors
/// Writing to `out` fails.
pub fn report(probe: &Probe, out: &mut dyn Write) -> io::Result<i32> {
    let caps = &probe.capabilities;
    writeln!(out, "loupe doctor")?;
    match &probe.typeset {
        Ok((w, h)) => writeln!(
            out,
            "  math font   {}: typesets (a sample formula drew {w}×{h} px)",
            probe.font
        )?,
        Err(e) => writeln!(out, "  math font   {}: not used: {e}", probe.font)?,
    }
    writeln!(
        out,
        "  graphics    {}",
        graphics_reason(&probe.environment, caps)
    )?;
    match (caps.cell_size_px, caps.text_area_px) {
        (Some((w, h)), _) => writeln!(out, "  cell size   {w}×{h} px")?,
        (None, _) => writeln!(
            out,
            "  cell size   not measured (assumed {}×{} px when drawing)",
            DEFAULT_CELL.0, DEFAULT_CELL.1
        )?,
    }
    let colour = |c: Option<Rgb>| c.map_or_else(|| "not measured".to_string(), hex);
    writeln!(
        out,
        "  colours     foreground {}, background {}",
        colour(caps.foreground),
        colour(caps.background)
    )?;
    let graphics = Graphics::from_capabilities(caps);
    let (status, line) = match (&probe.typeset, graphics) {
        (Ok(_), Some(g)) => (
            0,
            format!(
                "typeset as images ({}, drawn in {})",
                match g.protocol {
                    Protocol::Kitty => "kitty",
                    Protocol::Sixel => "sixel",
                },
                hex(g.foreground)
            ),
        ),
        (Ok(_), None) => (1, "shown as text: the terminal shows no images".to_string()),
        (Err(_), _) => (1, "shown as text: the math font cannot be used".to_string()),
    };
    writeln!(out, "  formulas    {line}")?;
    Ok(status)
}
