//! Which hosts to watch: a TOML file (`GAUGE_TARGETS`, default `config/targets.toml`).

use quench_serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum Kind {
    #[serde(rename = "pcp")]
    Pcp,
    #[serde(rename = "node")]
    Node,
}

impl Kind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pcp => "pcp",
            Self::Node => "node",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Target {
    pub name: String,
    pub kind: Kind,
    /// The base URL: `http://host:44322` for PCP, `http://host:9100` for `node_exporter`.
    pub url: String,
    /// A `node_exporter` on the same machine whose GPUs and temperatures are added to this host - for a PCP
    /// host whose PCP has no GPU module.
    #[serde(default)]
    pub also: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default = "default_interval")]
    pub interval_secs: u64,
    #[serde(default, rename = "target")]
    pub targets: Vec<Target>,
}

const fn default_interval() -> u64 {
    10
}

impl Config {
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut config: Self = quench_serde::toml::from_str(text).map_err(|e| e.to_string())?;
        config.interval_secs = config.interval_secs.clamp(1, 3600);
        let mut seen = std::collections::HashSet::new();
        for target in &mut config.targets {
            target.url = target.url.trim_end_matches('/').to_string();
            target.also = target
                .also
                .take()
                .map(|u| u.trim_end_matches('/').to_string());
            if target.name.trim().is_empty() || !seen.insert(target.name.clone()) {
                return Err(format!(
                    "target name `{}` is empty or repeated",
                    target.name
                ));
            }
        }
        Ok(config)
    }

    pub fn load() -> Result<Self, String> {
        let path = quench_core::env::get_or("GAUGE_TARGETS", "config/targets.toml");
        let text = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
        Self::parse(&text)
    }
}
