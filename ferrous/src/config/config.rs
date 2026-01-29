use anyhow::Context;
use colored::Colorize;
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

#[derive(Deserialize, Default, Clone, Debug)]
#[serde(default)]
pub struct Config {
    pub model: Option<String>,
    pub port: Option<u16>,
    #[serde(flatten)]
    pub sampling: SamplingConfig,
    pub debug: Option<bool>,
}

#[derive(Deserialize, Default, Clone, Debug)]
pub struct SamplingConfig {
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
}

impl Config {
    pub fn display(&self) {
        println!("{}", "Current configuration (merged):".bright_cyan().bold());
        println!();

        let fields = vec![
            ("model", self.model.as_ref().map(String::from)),
            ("port", self.port.map(|p| p.to_string())),
            (
                "temperature",
                self.sampling.temperature.map(|v| format!("{v:.2}")),
            ),
            ("top_p", self.sampling.top_p.map(|v| format!("{v:.2}"))),
            ("min_p", self.sampling.min_p.map(|v| format!("{v:.2}"))),
            ("top_k", self.sampling.top_k.map(|v| v.to_string())),
            (
                "repeat_penalty",
                self.sampling.repeat_penalty.map(|v| format!("{v:.2}")),
            ),
            ("context", self.sampling.context.map(|v| v.to_string())),
            (
                "max_tokens",
                self.sampling.max_tokens.map(|v| v.to_string()),
            ),
            ("mirostat", self.sampling.mirostat.map(|v| v.to_string())),
            (
                "mirostat_tau",
                self.sampling.mirostat_tau.map(|v| format!("{v:.3}")),
            ),
            (
                "mirostat_eta",
                self.sampling.mirostat_eta.map(|v| format!("{v:.3}")),
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

#[derive(Clone, Debug)]
struct ConfigField {
    name: &'static str,
    value: Option<String>,
    color: colored::Color,
}

impl ConfigField {
    const fn new(name: &'static str, value: Option<String>, color: colored::Color) -> Self {
        Self { name, value, color }
    }
}

/// Returns path to .ferrous/config.toml in current working directory
pub fn config_file_path() -> Result<PathBuf, anyhow::Error> {
    let cwd = std::env::current_dir().context("Cannot determine current working directory")?;

    Ok(cwd.join(".ferrous").join("config.toml"))
}

#[must_use]
/// Loads config.toml from current directory (if exists)
pub fn load() -> Config {
    let Ok(config_path) = config_file_path() else {
        return Config::default();
    };

    if !config_path.exists() {
        return Config::default();
    }

    match fs::read_to_string(&config_path) {
        Ok(content) => toml::from_str(&content).unwrap_or_else(|e| {
            eprintln!(
                "{} Invalid .ferrous/config.toml: {e}. Using defaults.",
                "Warning:".yellow().bold()
            );
            Config::default()
        }),
        Err(e) => {
            eprintln!(
                "{} Failed to read .ferrous/config.toml: {e}. Using defaults.",
                "Warning:".yellow().bold()
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
        && config.sampling.temperature.is_none()
        && config.sampling.top_p.is_none()
        && config.sampling.min_p.is_none()
        && config.sampling.top_k.is_none()
        && config.sampling.repeat_penalty.is_none()
        && config.sampling.context.is_none()
        && config.sampling.max_tokens.is_none()
        && config.sampling.mirostat.is_none()
        && config.sampling.mirostat_tau.is_none()
        && config.sampling.mirostat_eta.is_none()
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
            config.sampling.temperature.map(|v| format!("{v:.3}")),
            colored::Color::BrightYellow,
        ),
        ConfigField::new(
            "top_p",
            config.sampling.top_p.map(|v| format!("{v:.3}")),
            colored::Color::BrightYellow,
        ),
        ConfigField::new(
            "min_p",
            config.sampling.min_p.map(|v| format!("{v:.3}")),
            colored::Color::BrightYellow,
        ),
        ConfigField::new(
            "top_k",
            config.sampling.top_k.map(|v| v.to_string()),
            colored::Color::BrightGreen,
        ),
        ConfigField::new(
            "repeat_penalty",
            config.sampling.repeat_penalty.map(|v| format!("{v:.3}")),
            colored::Color::BrightYellow,
        ),
        ConfigField::new(
            "context",
            config.sampling.context.map(|v| v.to_string()),
            colored::Color::BrightGreen,
        ),
        ConfigField::new(
            "max_tokens",
            config.sampling.max_tokens.map(|v| v.to_string()),
            colored::Color::BrightGreen,
        ),
        ConfigField::new(
            "mirostat",
            config.sampling.mirostat.map(|v| v.to_string()),
            colored::Color::BrightGreen,
        ),
        ConfigField::new(
            "mirostat_tau",
            config.sampling.mirostat_tau.map(|v| format!("{v:.3}")),
            colored::Color::BrightYellow,
        ),
        ConfigField::new(
            "mirostat_eta",
            config.sampling.mirostat_eta.map(|v| format!("{v:.3}")),
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
