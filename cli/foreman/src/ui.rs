//! Output.
//!
//! One line per thing that happened, the label in a fixed column so a start
//! reads as a column of services rather than a wall of prose. The label is a badge from forge-tui, so foreman looks
//! like the other forge tools and the web apps.

use crate::config::ToneName;
use forge_tui::console;

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

impl From<Tone> for console::Tone {
    fn from(tone: Tone) -> Self {
        match tone {
            Tone::Info => Self::Info,
            Tone::Ok => Self::Success,
            Tone::Warn => Self::Warn,
            Tone::Error => Self::Error,
        }
    }
}

pub fn say(tone: Tone, label: &str, message: impl AsRef<str>) {
    println!(
        "{} {}",
        console::chip(tone.into(), label, LABEL_WIDTH),
        message.as_ref()
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
    println!(
        "  {} {url}",
        console::strong(&format!("{name:<LABEL_WIDTH$}"))
    );
}

pub fn dim(message: impl AsRef<str>) {
    println!("{}", console::muted(message.as_ref()));
}

pub fn blank() {
    println!();
}

/// Indented tail of a log, for a service that died on the way up.
pub fn quote(text: &str) {
    for line in text.lines() {
        println!("{}", console::muted(&format!("             {line}")));
    }
}
