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
    pub debug: Option<bool>,
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
        && config.debug.is_none()
    {
        println!("{}", "No custom settings found in .ferrous.toml".dimmed());
        return;
    }

    println!("{}", "Loaded from .ferrous.toml:".bright_black().bold());

    if let Some(v) = &config.model {
        println!("  {:<14} = {}", "model".bright_blue(), v.bright_cyan());
    }
    if let Some(v) = config.port {
        println!(
            "  {:<14} = {}",
            "port".bright_blue(),
            v.to_string().bright_green()
        );
    }
    if let Some(v) = config.temperature {
        println!(
            "  {:<14} = {}",
            "temperature".bright_blue(),
            format!("{:.3}", v).bright_yellow()
        );
    }
    if let Some(v) = config.top_p {
        println!(
            "  {:<14} = {}",
            "top_p".bright_blue(),
            format!("{:.3}", v).bright_yellow()
        );
    }
    if let Some(v) = config.min_p {
        println!(
            "  {:<14} = {}",
            "min_p".bright_blue(),
            format!("{:.3}", v).bright_yellow()
        );
    }
    if let Some(v) = config.top_k {
        println!(
            "  {:<14} = {}",
            "top_k".bright_blue(),
            v.to_string().bright_green()
        );
    }
    if let Some(v) = config.repeat_penalty {
        println!(
            "  {:<14} = {}",
            "repeat_penalty".bright_blue(),
            format!("{:.3}", v).bright_yellow()
        );
    }
    if let Some(v) = config.max_tokens {
        println!(
            "  {:<14} = {}",
            "max_tokens".bright_blue(),
            v.to_string().bright_green()
        );
    }
    if let Some(v) = config.debug {
        println!(
            "  {:<14} = {}",
            "debug".bright_blue(),
            if v {
                "true".bright_green()
            } else {
                "false".bright_red()
            }
        );
    }

    println!();
}
