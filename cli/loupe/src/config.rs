//! `~/.config/loupe/config.toml` (or under `$XDG_CONFIG_HOME`): defaults for the flags. A flag on the command
//! line wins over the file.
//!
//! ```toml
//! math_font = "~/fonts/SomeMath.ttf"   # a TrueType font with a MATH table, instead of the bundled one
//! footnotes = "inline"                 # end | inline
//! math = "text"                        # source | text
//! images = "off"                       # on | off
//! width = 100
//! warehouse_url = "https://host/warehouse"   # where released books are (`loupe --book`)
//! storage = "docs"                         # the files storage they are in
//! gatehouse_url = "https://host/gatehouse" # for `loupe login`
//! ```

use crate::cli::{Cli, Footnotes, Images, Math};
use quench_serde::Value;
use std::path::{Path, PathBuf};

/// What the config file says; every key may be left out.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Config {
    pub math_font: Option<PathBuf>,
    pub footnotes: Option<Footnotes>,
    pub math: Option<Math>,
    pub images: Option<Images>,
    pub width: Option<usize>,
    pub warehouse_url: Option<String>,
    pub storage: Option<String>,
    pub gatehouse_url: Option<String>,
}

/// Where the config file is: `$XDG_CONFIG_HOME/loupe/config.toml` when that is set (and absolute), else
/// `$HOME/.config/loupe/config.toml`; `None` with neither.
#[must_use]
pub fn path(xdg_config_home: Option<&str>, home: Option<&str>) -> Option<PathBuf> {
    if let Some(xdg) = xdg_config_home.filter(|x| Path::new(x).is_absolute()) {
        return Some(Path::new(xdg).join("loupe").join("config.toml"));
    }
    home.filter(|h| !h.is_empty()).map(|h| {
        Path::new(h)
            .join(".config")
            .join("loupe")
            .join("config.toml")
    })
}

/// `~/x` with the home directory, other paths as they are; a relative path is taken from the config file's
/// directory.
fn expand(path: &str, home: Option<&str>, base: &Path) -> PathBuf {
    if let (Some(rest), Some(home)) = (path.strip_prefix("~/"), home) {
        return Path::new(home).join(rest);
    }
    let path = Path::new(path);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    }
}

fn choice<T>(value: &Value, key: &str, options: &[(&str, T)]) -> Result<T, String>
where
    T: Copy,
{
    let text = value
        .as_str()
        .ok_or_else(|| format!("`{key}` must be a string"))?;
    options
        .iter()
        .find(|(name, _)| *name == text)
        .map(|&(_, v)| v)
        .ok_or_else(|| {
            let names: Vec<&str> = options.iter().map(|(n, _)| *n).collect();
            format!("`{key} = \"{text}\"`: expected one of {}", names.join(", "))
        })
}

impl Config {
    /// Reads the text of the config file at `file`. `home` expands `~/` in `math_font`.
    ///
    /// # Errors
    /// The text is not TOML, a key is unknown, or a value is of the wrong kind.
    pub fn parse(text: &str, file: &Path, home: Option<&str>) -> Result<Self, String> {
        let value: Value = quench_serde::toml::from_str(text).map_err(|e| e.to_string())?;
        let Value::Object(map) = &value else {
            return Err("not a table of settings".to_string());
        };
        let base = file.parent().unwrap_or_else(|| Path::new(""));
        let mut config = Self::default();
        for (key, value) in map.iter() {
            match key.as_str() {
                "math_font" => {
                    let path = value
                        .as_str()
                        .ok_or("`math_font` must be a string (a path)")?;
                    config.math_font = Some(expand(path, home, base));
                }
                "footnotes" => {
                    config.footnotes = Some(choice(
                        value,
                        key,
                        &[("end", Footnotes::End), ("inline", Footnotes::Inline)],
                    )?);
                }
                "math" => {
                    config.math = Some(choice(
                        value,
                        key,
                        &[("source", Math::Source), ("text", Math::Text)],
                    )?);
                }
                "images" => {
                    config.images = Some(choice(
                        value,
                        key,
                        &[("on", Images::On), ("off", Images::Off)],
                    )?);
                }
                "width" => {
                    let width = value
                        .as_i64()
                        .and_then(|w| usize::try_from(w).ok())
                        .filter(|&w| w > 0)
                        .ok_or("`width` must be a whole number of columns, at least 1")?;
                    config.width = Some(width);
                }
                "warehouse_url" | "storage" | "gatehouse_url" => {
                    let text = value
                        .as_str()
                        .filter(|t| !t.trim().is_empty())
                        .ok_or_else(|| format!("`{key}` must be a non-empty string"))?
                        .to_string();
                    match key.as_str() {
                        "warehouse_url" => config.warehouse_url = Some(text),
                        "storage" => config.storage = Some(text),
                        _ => config.gatehouse_url = Some(text),
                    }
                }
                other => {
                    return Err(format!(
                        "unknown setting `{other}` (known: math_font, footnotes, math, images, width, \
                         warehouse_url, storage, gatehouse_url)"
                    ));
                }
            }
        }
        Ok(config)
    }
}

/// The settings in effect: the command line over the config file over the defaults.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Effective {
    pub math_font: Option<PathBuf>,
    pub footnotes: Footnotes,
    pub math: Option<Math>,
    pub images: Images,
    pub width: Option<usize>,
}

impl Effective {
    #[must_use]
    pub fn of(cli: &Cli, config: &Config) -> Self {
        Self {
            math_font: cli.math_font.clone().or_else(|| config.math_font.clone()),
            footnotes: cli.footnotes.or(config.footnotes).unwrap_or(Footnotes::End),
            math: cli.math.or(config.math),
            images: cli.images.or(config.images).unwrap_or(Images::On),
            width: cli.width.or(config.width),
        }
    }
}
