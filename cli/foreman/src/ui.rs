//! Output.
//!
//! One line per thing that happened, the label in a fixed column so a start
//! reads as a column of services rather than a wall of prose. Colours come from
//! quench-cli so foreman looks like the rest of the toolchain.

use crate::config::ToneName;
use quench_cli::prelude::{CYAN, DIM, GREEN, RESET, WHITE, YELLOW};

pub const RED: &str = "\x1b[31m";

/// Width of the label column. Long enough for the service names anyone
/// actually types.
const LABEL_WIDTH: usize = 12;

#[derive(Clone, Copy, Debug)]
pub enum Tone {
    Info,
    Ok,
    Warn,
    Error,
}

impl From<ToneName> for Tone {
    fn from(tone: ToneName) -> Self {
        match tone {
            ToneName::Info => Self::Info,
            ToneName::Ok => Self::Ok,
            ToneName::Warn => Self::Warn,
            ToneName::Error => Self::Error,
        }
    }
}

impl Tone {
    #[must_use]
    pub const fn colour(self) -> &'static str {
        match self {
            Self::Info => CYAN,
            Self::Ok => GREEN,
            Self::Warn => YELLOW,
            Self::Error => RED,
        }
    }
}

pub fn say(tone: Tone, label: &str, message: impl AsRef<str>) {
    println!(
        "{}{:<width$}{} {}",
        tone.colour(),
        label,
        RESET,
        message.as_ref(),
        width = LABEL_WIDTH
    );
}

pub fn info(label: &str, message: impl AsRef<str>) {
    say(Tone::Info, label, message);
}

pub fn ok(label: &str, message: impl AsRef<str>) {
    say(Tone::Ok, label, message);
}

pub fn warn(label: &str, message: impl AsRef<str>) {
    say(Tone::Warn, label, message);
}

pub fn error(label: &str, message: impl AsRef<str>) {
    say(Tone::Error, label, message);
}

/// A service and its address, as printed in the summary and the picker.
pub fn entry(name: &str, url: &str) {
    println!("  {WHITE}{name:<LABEL_WIDTH$}{RESET} {url}");
}

pub fn dim(message: impl AsRef<str>) {
    println!("{DIM}{}{RESET}", message.as_ref());
}

pub fn blank() {
    println!();
}

/// Indented tail of a log, for a service that died on the way up.
pub fn quote(text: &str) {
    for line in text.lines() {
        println!("{DIM}             {line}{RESET}");
    }
}
