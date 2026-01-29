use anyhow::Context;
use colored::Colorize;
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

#[derive(Deserialize, Default, Clone)]
pub struct Config {
    pub model: Option<String>,
    pub port: Option<u16>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub min_p: Option<f32>,
    pub top_k: Option<i32>,
    pub repeat_penalty: Option<f32>,
    pub context: Option<u32>,
    pub max_tokens: Option<u32>,
    pub mirostat: Option<i32>,
    pub mirostat_tau: Option<f32>,
    pub mirostat_eta: Option<f32>,
    pub debug: Option<bool>,
}

impl Config {
    pub fn display(&self) {
        println!("{}", "Current configuration (merged):".bright_cyan().bold());
        println!();

        let fields = vec![
            ("model", self.model.as_ref().map(String::from)),
            ("port", self.port.map(|p| p.to_string())),
            ("temperature", self.temperature.map(|v| format!("{:.2}", v))),
            ("top_p", self.top_p.map(|v| format!("{:.2}", v))),
            ("min_p", self.min_p.map(|v| format!("{:.2}", v))),
            ("top_k", self.top_k.map(|v| v.to_string())),
            (
                "repeat_penalty",
                self.repeat_penalty.map(|v| format!("{:.2}", v)),
            ),
            ("context", self.context.map(|v| v.to_string())),
            ("max_tokens", self.max_tokens.map(|v| v.to_string())),
            ("mirostat", self.mirostat.map(|v| v.to_string())),
            (
                "mirostat_tau",
                self.mirostat_tau.map(|v| format!("{:.3}", v)),
            ),
            (
                "mirostat_eta",
                self.mirostat_eta.map(|v| format!("{:.3}", v)),
            ),
            ("debug", self.debug.map(|v| v.to_string())),
        ];

        let mut any_shown = false;

        for (name, value) in fields {
            if let Some(val) = value {
                println!("  {:<14} = {}", name.bright_blue(), val.bright_white());
                any_shown = true;
            }
        }

        if !any_shown {
            println!("{}", "  (all values at default)".dimmed());
        }

        println!();
    }
}

#[derive(Clone)]
struct ConfigField {
    name: &'static str,
    value: Option<String>,
    color: colored::Color,
}

impl ConfigField {
    fn new(name: &'static str, value: Option<String>, color: colored::Color) -> Self {
        Self { name, value, color }
    }
}

/// Returns path to .ferrous/config.toml in current working directory
pub fn config_file_path() -> Result<PathBuf, anyhow::Error> {
    let cwd = std::env::current_dir().context("Cannot determine current working directory")?;

    Ok(cwd.join(".ferrous").join("config.toml"))
}

/// Loads config.toml from current directory (if exists)
pub fn load() -> Config {
    let config_path = match config_file_path() {
        Ok(p) => p,
        Err(_) => return Config::default(),
    };

    if !config_path.exists() {
        return Config::default();
    }

    match fs::read_to_string(&config_path) {
        Ok(content) => toml::from_str(&content).unwrap_or_else(|e| {
            eprintln!(
                "{} Invalid .ferrous/config.toml: {}. Using defaults.",
                "Warning:".yellow().bold(),
                e
            );
            Config::default()
        }),
        Err(e) => {
            eprintln!(
                "{} Failed to read .ferrous/config.toml: {}. Using defaults.",
                "Warning:".yellow().bold(),
                e
            );
            Config::default()
        }
    }
}

/// Prints which settings were actually loaded from config.toml
/// (only non-None fields are shown, with colors for better readability)
pub fn print_loaded(config: &Config, is_debug: bool) {
    if !is_debug {
        return;
    }

    // Early exit if nothing is set
    if config.model.is_none()
        && config.port.is_none()
        && config.temperature.is_none()
        && config.top_p.is_none()
        && config.min_p.is_none()
        && config.top_k.is_none()
        && config.repeat_penalty.is_none()
        && config.context.is_none()
        && config.max_tokens.is_none()
        && config.mirostat.is_none()
        && config.mirostat_tau.is_none()
        && config.mirostat_eta.is_none()
        && config.debug.is_none()
    {
        println!("{}", "No custom settings found in config.toml".dimmed());
        return;
    }

    println!("{}", "Loaded from config.toml:".bright_black().bold());

    let fields = vec![
        ConfigField::new("model", config.model.clone(), colored::Color::BrightCyan),
        ConfigField::new(
            "port",
            config.port.map(|v| v.to_string()),
            colored::Color::BrightGreen,
        ),
        ConfigField::new(
            "temperature",
            config.temperature.map(|v| format!("{:.3}", v)),
            colored::Color::BrightYellow,
        ),
        ConfigField::new(
            "top_p",
            config.top_p.map(|v| format!("{:.3}", v)),
            colored::Color::BrightYellow,
        ),
        ConfigField::new(
            "min_p",
            config.min_p.map(|v| format!("{:.3}", v)),
            colored::Color::BrightYellow,
        ),
        ConfigField::new(
            "top_k",
            config.top_k.map(|v| v.to_string()),
            colored::Color::BrightGreen,
        ),
        ConfigField::new(
            "repeat_penalty",
            config.repeat_penalty.map(|v| format!("{:.3}", v)),
            colored::Color::BrightYellow,
        ),
        ConfigField::new(
            "context",
            config.context.map(|v| v.to_string()),
            colored::Color::BrightGreen,
        ),
        ConfigField::new(
            "max_tokens",
            config.max_tokens.map(|v| v.to_string()),
            colored::Color::BrightGreen,
        ),
        ConfigField::new(
            "mirostat",
            config.mirostat.map(|v| v.to_string()),
            colored::Color::BrightGreen,
        ),
        ConfigField::new(
            "mirostat_tau",
            config.mirostat_tau.map(|v| format!("{:.3}", v)),
            colored::Color::BrightYellow,
        ),
        ConfigField::new(
            "mirostat_eta",
            config.mirostat_eta.map(|v| format!("{:.3}", v)),
            colored::Color::BrightYellow,
        ),
        ConfigField::new(
            "debug",
            config.debug.map(|v| v.to_string()),
            colored::Color::BrightGreen,
        ),
    ];

    for field in fields {
        if let Some(value) = field.value {
            println!(
                "  {:<14} = {}",
                field.name.bright_blue(),
                value.color(field.color)
            );
        }
    }

    println!();
}
