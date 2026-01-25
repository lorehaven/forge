use colored::Colorize;
use serde::Deserialize;
use std::fs;

#[derive(Deserialize, Default, Clone)]
pub struct Config {
    pub model: Option<String>,
    pub port: Option<u16>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub min_p: Option<f32>,
    pub top_k: Option<i32>,
    pub repeat_penalty: Option<f32>,
    pub max_tokens: Option<u32>,
    pub mirostat: Option<i32>,
    pub mirostat_tau: Option<f32>,
    pub mirostat_eta: Option<f32>,
    pub debug: Option<bool>,
}

#[derive(Clone)]
struct ConfigField {
    name: &'static str,
    value: Option<String>,
    color: colored::Color,
}

impl ConfigField {
    fn new(name: &'static str, value: Option<String>, color: colored::Color) -> Self {
        ConfigField { name, value, color }
    }
}

/// Loads .ferrous.toml from current directory (if exists)
pub fn load() -> Config {
    let config_path = match std::env::current_dir() {
        Ok(dir) => dir.join(".ferrous.toml"),
        Err(_) => return Config::default(),
    };

    if !config_path.exists() {
        return Config::default();
    }

    match fs::read_to_string(&config_path) {
        Ok(content) => toml::from_str(&content).unwrap_or_else(|e| {
            eprintln!(
                "{} Invalid .ferrous.toml: {}. Using defaults.",
                "Warning:".yellow().bold(),
                e
            );
            Config::default()
        }),
        Err(e) => {
            eprintln!(
                "{} Failed to read .ferrous.toml: {}. Using defaults.",
                "Warning:".yellow().bold(),
                e
            );
            Config::default()
        }
    }
}

/// Prints which settings were actually loaded from .ferrous.toml  
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
        && config.max_tokens.is_none()
        && config.mirostat.is_none()
        && config.mirostat_tau.is_none()
        && config.mirostat_eta.is_none()
        && config.debug.is_none()
    {
        println!("{}", "No custom settings found in .ferrous.toml".dimmed());
        return;
    }

    println!("{}", "Loaded from .ferrous.toml:".bright_black().bold());

    let fields = vec![
        ConfigField::new("model", config.model.clone(), colored::Color::BrightCyan),
        ConfigField::new("port", config.port.map(|v| v.to_string()), colored::Color::BrightGreen),
        ConfigField::new("temperature", config.temperature.map(|v| format!("{:.3}", v)), colored::Color::BrightYellow),
        ConfigField::new("top_p", config.top_p.map(|v| format!("{:.3}", v)), colored::Color::BrightYellow),
        ConfigField::new("min_p", config.min_p.map(|v| format!("{:.3}", v)), colored::Color::BrightYellow),
        ConfigField::new("top_k", config.top_k.map(|v| v.to_string()), colored::Color::BrightGreen),
        ConfigField::new("repeat_penalty", config.repeat_penalty.map(|v| format!("{:.3}", v)), colored::Color::BrightYellow),
        ConfigField::new("max_tokens", config.max_tokens.map(|v| v.to_string()), colored::Color::BrightGreen),
        ConfigField::new("mirostat", config.mirostat.map(|v| v.to_string()), colored::Color::BrightGreen),
        ConfigField::new("mirostat_tau", config.mirostat_tau.map(|v| format!("{:.3}", v)), colored::Color::BrightYellow),
        ConfigField::new("mirostat_eta", config.mirostat_eta.map(|v| format!("{:.3}", v)), colored::Color::BrightYellow),
        ConfigField::new("debug", config.debug.map(|v| if v { "true".to_string() } else { "false".to_string() }), colored::Color::BrightGreen),
    ];

    for field in fields {
        if let Some(value) = field.value {
            println!("  {:<14} = {}", field.name.bright_blue(), value.color(field.color));
        }
    }

    println!();
}