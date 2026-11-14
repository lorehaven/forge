use forge_tui::console::{Tone, print_box_banner, print_status};
use quench_cli::prelude::print_line;

pub fn banner() {
    print_box_banner("Warehouse CLI", "docker crates files admin");
}

pub fn info(message: impl AsRef<str>) {
    print_status(Tone::Info, "warehouse", message.as_ref());
}

pub fn ok(message: impl AsRef<str>) {
    print_status(Tone::Success, "warehouse", message.as_ref());
}

pub fn warn(message: impl AsRef<str>) {
    print_status(Tone::Warn, "warehouse", message.as_ref());
}

pub fn error(message: impl AsRef<str>) {
    print_status(Tone::Error, "warehouse", message.as_ref());
}

pub fn line(message: impl AsRef<str>) {
    print_line(message);
}
