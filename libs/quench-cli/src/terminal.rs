use crossterm::style::Stylize;
use std::io::Write;

pub enum Tone {
    Info,
    Success,
    Warn,
    Error,
}

pub const RESET: &str = "\x1b[0m";
pub const BOLD: &str = "\x1b[1m";
pub const DIM: &str = "\x1b[2m";
pub const CYAN: &str = "\x1b[36m";
pub const BLUE: &str = "\x1b[34m";
pub const GREEN: &str = "\x1b[32m";
pub const YELLOW: &str = "\x1b[33m";
pub const WHITE: &str = "\x1b[37m";

pub const SEP: &str = "────────────────────────────────────────────────────────";
pub const SEP_THIN: &str = "┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄";

pub fn print_box_banner(title: &str, subtitle: &str) {
    let border = "┌─────────────────────────────────────────────┐";
    let bottom = "└─────────────────────────────────────────────┘";

    println!("{}", border.dark_cyan());
    println!("{}", format!("│ {:<43} │", title).white().bold());
    println!("{}", format!("│ {:<43} │", subtitle).dark_grey());
    println!("{}", bottom.dark_cyan());
}

pub fn print_status(tone: Tone, label: &str, message: &str) {
    let styled_label = match tone {
        Tone::Info => label.cyan().bold(),
        Tone::Success => label.green().bold(),
        Tone::Warn => label.yellow().bold(),
        Tone::Error => label.red().bold(),
    };

    println!("{} {}", styled_label, message.white());
}

pub fn print_component_preview(name: &str, description: &str) {
    let head = format!("[{name}]").cyan().bold();
    println!("{} {}", head, description.white());
}

pub fn repl_prompt(app: &str, context: &str) -> String {
    format!("{}({})> ", app.cyan().bold(), context.dark_grey())
}

pub fn print_line(message: impl AsRef<str>) {
    println!("{}", message.as_ref());
}

pub fn print_error_line(message: impl AsRef<str>) {
    eprintln!("{}", message.as_ref());
}

pub fn print_inline(message: impl AsRef<str>) {
    print!("{}", message.as_ref());
    let _ = std::io::stdout().flush();
}
