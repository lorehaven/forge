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

#[derive(Clone, Copy)]
pub enum Tone {
    Info,
    Ok,
    Warn,
    Error,
}

impl From<ToneName> for Tone {
    fn from(tone: ToneName) -> Self {
        match tone {
            ToneName::Info => Tone::Info,
            ToneName::Ok => Tone::Ok,
            ToneName::Warn => Tone::Warn,
            ToneName::Error => Tone::Error,
        }
    }
}

impl Tone {
    fn colour(self) -> &'static str {
        match self {
            Tone::Info => CYAN,
            Tone::Ok => GREEN,
            Tone::Warn => YELLOW,
            Tone::Error => RED,
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
