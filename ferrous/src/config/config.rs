use anyhow::Context;
use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum ModelBackend {
    LocalLlama {
        model_path: String,
        #[serde(default = "default_port")]
        port: u16,
        #[serde(default = "default_context_size")]
        context_size: u32,
        #[serde(default = "default_num_gpu_layers")]
        num_gpu_layers: u16,
    },
    OpenAi {
        model_name: String,
        api_key: Option<String>,
        api_base: Option<String>,
    },
    Anthropic {
        model_name: String,
        api_key: Option<String>,
    },
    External {
        api_base: String,
        api_key: Option<String>,
        model_name: Option<String>,
    },
}

const fn default_port() -> u16 {
    8080
}
const fn default_context_size() -> u32 {
    8192
}
const fn default_num_gpu_layers() -> u16 {
    999
}

#[derive(Deserialize, Serialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ModelRole {
    Chat,
    Planner,
    Embedding,
}

#[derive(Deserialize, Serialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum UiMode {
    #[default]
    Repl,
    Web,
}

#[derive(Deserialize, Serialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum UiTheme {
    #[default]
    Dark,
    Light,
}

#[derive(Deserialize, Serialize, Default, Clone, Debug)]
#[serde(default)]
pub struct Config {
    pub base_model_path: Option<String>,
    pub models: HashMap<ModelRole, ModelBackend>,
    pub sampling: SamplingConfig,
    pub shell: Option<ShellConfig>,
    pub debug: Option<bool>,
    pub ui: Option<UiMode>,
    pub theme: Option<UiTheme>,
}

#[derive(Deserialize, Serialize, Default, Clone, Debug)]
pub struct ShellConfig {
    pub allowed_prefixes: Option<Vec<String>>,
}

#[derive(Deserialize, Serialize, Default, Clone, Debug)]
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

        if !self.models.is_empty() {
            println!("{}", "Models by role:".bright_yellow());
            for (role, backend) in &self.models {
                let role_str = match role {
                    ModelRole::Chat => "chat",
                    ModelRole::Planner => "planner",
                    ModelRole::Embedding => "embedding",
                };
                let backend_info = match backend {
                    ModelBackend::LocalLlama {
                        model_path, port, ..
                    } => {
                        let full_path = if !model_path.starts_with('/')
                            && !model_path.starts_with('.')
                            && let Some(ref base) = self.base_model_path
                        {
                            format!("{}/{}", base.trim_end_matches('/'), model_path)
                        } else {
                            model_path.clone()
                        };
                        format!("Local Llama (port {port}): {full_path}")
                    }
                    ModelBackend::OpenAi { model_name, .. } => format!("OpenAI: {model_name}"),
                    ModelBackend::Anthropic { model_name, .. } => {
                        format!("Anthropic: {model_name}")
                    }
                    ModelBackend::External { api_base, .. } => format!("External: {api_base}"),
                };
                println!(
                    "  {:<14} = {}",
                    role_str.bright_blue(),
                    backend_info.bright_white()
                );
            }
            println!();
        }

        let fields = vec![
            (
                "ui",
                self.ui.map(|v| match v {
                    UiMode::Repl => "repl".to_string(),
                    UiMode::Web => "web".to_string(),
                }),
            ),
            (
                "theme",
                self.theme.map(|v| match v {
                    UiTheme::Dark => "dark".to_string(),
                    UiTheme::Light => "light".to_string(),
                }),
            ),
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
            (
                "shell_allowlist",
                self.shell.as_ref().and_then(|s| {
                    s.allowed_prefixes
                        .as_ref()
                        .map(|prefixes| format!("{} entries", prefixes.len()))
                }),
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

pub fn save(config: &Config) -> Result<(), anyhow::Error> {
    let config_path = config_file_path()?;
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create config directory {}", parent.display()))?;
    }
    let content = toml::to_string_pretty(config).context("Failed to serialize config.toml")?;
    fs::write(&config_path, content)
        .with_context(|| format!("Failed to write {}", config_path.display()))
}

/// Prints which settings were actually loaded from config.toml
/// (only non-None fields are shown, with colors for better readability)
#[allow(clippy::too_many_lines)]
pub fn print_loaded(config: &Config, is_debug: bool) {
    if !is_debug {
        return;
    }

    // Early exit if nothing is set
    if config.models.is_empty()
        && config.ui.is_none()
        && config.theme.is_none()
        && config.shell.is_none()
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

    if let Some(ref base) = config.base_model_path {
        println!(
            "  {:<14} = {}",
            "base_path".bright_blue(),
            base.bright_white()
        );
    }

    if !config.models.is_empty() {
        for (role, backend) in &config.models {
            let role_str = match role {
                ModelRole::Chat => "chat",
                ModelRole::Planner => "planner",
                ModelRole::Embedding => "embedding",
            };
            println!("  {:<14} = {:?}", role_str.bright_blue(), backend);
        }
    }

    let fields = vec![
        ConfigField::new(
            "ui",
            config.ui.map(|v| match v {
                UiMode::Repl => "repl".to_string(),
                UiMode::Web => "web".to_string(),
            }),
            colored::Color::BrightGreen,
        ),
        ConfigField::new(
            "theme",
            config.theme.map(|v| match v {
                UiTheme::Dark => "dark".to_string(),
                UiTheme::Light => "light".to_string(),
            }),
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
            "shell_allowlist",
            config.shell.as_ref().and_then(|s| {
                s.allowed_prefixes
                    .as_ref()
                    .map(|prefixes| format!("{} entries", prefixes.len()))
            }),
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
