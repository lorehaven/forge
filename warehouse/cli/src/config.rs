use crate::domain::{
    RegistryConfig, RootConfig, merge_root_config, normalize_path, validate_registry_name,
};
use anyhow::{Context, Result, anyhow, bail};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistrySource {
    Local,
    Global,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigScope {
    Local,
    Global,
}

#[derive(Debug)]
pub struct RegistryEntry {
    pub name: String,
    pub source: RegistrySource,
    pub config: RegistryConfig,
}

pub struct ConfigStore {
    local_root: PathBuf,
    global_root: Option<PathBuf>,
}

impl ConfigStore {
    pub fn new() -> Self {
        Self {
            local_root: PathBuf::from(".warehouse"),
            global_root: default_global_root(),
        }
    }

    pub fn ensure_local_layout(&self) -> Result<()> {
        fs::create_dir_all(self.local_registries_dir())
            .context("failed to create .warehouse/registries")?;
        Ok(())
    }

    pub fn ensure_layout(&self, scope: ConfigScope) -> Result<()> {
        match scope {
            ConfigScope::Local => self.ensure_local_layout(),
            ConfigScope::Global => {
                let Some(dir) = self.global_registries_dir() else {
                    bail!("HOME is not set; cannot use global config");
                };
                fs::create_dir_all(dir).context("failed to create ~/.config/warehouse/registries")
            }
        }
    }

    pub fn load_effective_root_config(&self) -> Result<RootConfig> {
        let global = self.load_global_root_config()?;
        let local = self.load_local_root_config()?;
        Ok(merge_root_config(global, local))
    }

    pub fn save_root_config(&self, scope: ConfigScope, cfg: &RootConfig) -> Result<()> {
        self.ensure_layout(scope)?;
        let path = self.root_config_path(scope)?;
        let content = toml::to_string_pretty(cfg).context("failed to serialize root config")?;
        fs::write(&path, content).with_context(|| format!("failed to write {}", path.display()))
    }

    pub fn resolve_registry_name(&self, requested: Option<String>) -> Result<String> {
        if let Some(name) = requested {
            validate_registry_name(&name)?;
            if self.registry_exists_effective(&name) {
                return Ok(name);
            }
            bail!("registry '{}' does not exist", name);
        }

        let cfg = self.load_effective_root_config()?;
        let name = cfg.docker.current_registry.ok_or_else(|| {
            anyhow!(
                "no active registry configured; use `warehouse docker registry add ... --use` or `warehouse docker registry use ...`"
            )
        })?;

        validate_registry_name(&name)?;
        if self.registry_exists_effective(&name) {
            Ok(name)
        } else {
            bail!("active registry '{}' does not exist", name)
        }
    }

    pub fn registry_exists_effective(&self, name: &str) -> bool {
        self.local_registry_file_path(name).exists()
            || self
                .global_registry_file_path(name)
                .is_some_and(|p| p.exists())
    }

    pub fn registry_exists_in_scope(&self, scope: ConfigScope, name: &str) -> bool {
        match self.registry_file_path(scope, name) {
            Ok(path) => path.exists(),
            Err(_) => false,
        }
    }

    pub fn load_effective_registry(&self, name: &str) -> Result<RegistryEntry> {
        validate_registry_name(name)?;

        let local_path = self.local_registry_file_path(name);
        if local_path.exists() {
            let config = load_registry_from_path(&local_path)?;
            return Ok(RegistryEntry {
                name: name.to_string(),
                source: RegistrySource::Local,
                config,
            });
        }

        if let Some(global_path) = self.global_registry_file_path(name)
            && global_path.exists()
        {
            let config = load_registry_from_path(&global_path)?;
            return Ok(RegistryEntry {
                name: name.to_string(),
                source: RegistrySource::Global,
                config,
            });
        }

        bail!("registry '{}' does not exist", name)
    }

    pub fn load_effective_registry_optional(&self, name: &str) -> Result<Option<RegistryEntry>> {
        validate_registry_name(name)?;
        if !self.registry_exists_effective(name) {
            return Ok(None);
        }

        self.load_effective_registry(name).map(Some)
    }

    pub fn save_registry(
        &self,
        scope: ConfigScope,
        name: &str,
        cfg: &RegistryConfig,
    ) -> Result<()> {
        validate_registry_name(name)?;
        self.ensure_layout(scope)?;
        let path = self.registry_file_path(scope, name)?;

        let mut normalized = cfg.clone();
        normalized.docker.path = normalize_path(&normalized.docker.path);

        let content =
            toml::to_string_pretty(&normalized).context("failed to serialize registry config")?;
        fs::write(&path, content).with_context(|| format!("failed to write {}", path.display()))
    }

    pub fn remove_registry(&self, scope: ConfigScope, name: &str) -> Result<()> {
        validate_registry_name(name)?;
        let path = self.registry_file_path(scope, name)?;
        if !path.exists() {
            let scope_dir = self.registries_dir(scope)?;
            bail!(
                "registry '{}' does not exist in {}",
                name,
                scope_dir.display()
            );
        }
        fs::remove_file(&path)
            .with_context(|| format!("failed to remove registry file {}", path.display()))
    }

    pub fn list_effective_registries(&self) -> Result<Vec<RegistryEntry>> {
        let mut source_map: BTreeMap<String, RegistrySource> = BTreeMap::new();

        for name in self.list_registry_names_in_dir(self.global_registries_dir())? {
            source_map.insert(name, RegistrySource::Global);
        }
        for name in self.list_registry_names_in_dir(Some(self.local_registries_dir()))? {
            source_map.insert(name, RegistrySource::Local);
        }

        let mut entries = Vec::with_capacity(source_map.len());
        for (name, _) in source_map {
            entries.push(self.load_effective_registry(&name)?);
        }

        Ok(entries)
    }

    fn load_local_root_config(&self) -> Result<RootConfig> {
        self.load_root_config_from_path(self.local_root_config_path())
    }

    fn load_global_root_config(&self) -> Result<RootConfig> {
        self.load_root_config_from_path_option(self.global_root_config_path())
    }

    fn load_root_config_from_path(&self, path: PathBuf) -> Result<RootConfig> {
        if !path.exists() {
            return Ok(RootConfig::default());
        }

        let content = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        toml::from_str(&content).with_context(|| format!("failed to parse {}", path.display()))
    }

    fn load_root_config_from_path_option(&self, path: Option<PathBuf>) -> Result<RootConfig> {
        let Some(path) = path else {
            return Ok(RootConfig::default());
        };
        self.load_root_config_from_path(path)
    }

    fn list_registry_names_in_dir(&self, dir: Option<PathBuf>) -> Result<Vec<String>> {
        let Some(dir) = dir else {
            return Ok(Vec::new());
        };
        if !dir.exists() {
            return Ok(Vec::new());
        }

        let mut names = Vec::new();
        for entry in
            fs::read_dir(&dir).with_context(|| format!("failed to read {}", dir.display()))?
        {
            let path = entry?.path();
            if path.extension().and_then(|x| x.to_str()) != Some("toml") {
                continue;
            }
            if let Some(stem) = path.file_stem().and_then(|x| x.to_str()) {
                names.push(stem.to_string());
            }
        }

        Ok(names)
    }

    fn local_root_config_path(&self) -> PathBuf {
        self.local_root.join("config.toml")
    }

    fn global_root_config_path(&self) -> Option<PathBuf> {
        self.global_root.as_ref().map(|p| p.join("config.toml"))
    }

    fn local_registries_dir(&self) -> PathBuf {
        self.local_root.join("registries")
    }

    fn global_registries_dir(&self) -> Option<PathBuf> {
        self.global_root.as_ref().map(|p| p.join("registries"))
    }

    fn local_registry_file_path(&self, name: &str) -> PathBuf {
        self.local_root
            .join("registries")
            .join(format!("{name}.toml"))
    }

    fn global_registry_file_path(&self, name: &str) -> Option<PathBuf> {
        self.global_root
            .as_ref()
            .map(|root| root.join("registries").join(format!("{name}.toml")))
    }

    fn root_config_path(&self, scope: ConfigScope) -> Result<PathBuf> {
        match scope {
            ConfigScope::Local => Ok(self.local_root_config_path()),
            ConfigScope::Global => self
                .global_root_config_path()
                .ok_or_else(|| anyhow!("HOME is not set; cannot use global config")),
        }
    }

    fn registries_dir(&self, scope: ConfigScope) -> Result<PathBuf> {
        match scope {
            ConfigScope::Local => Ok(self.local_registries_dir()),
            ConfigScope::Global => self
                .global_registries_dir()
                .ok_or_else(|| anyhow!("HOME is not set; cannot use global config")),
        }
    }

    fn registry_file_path(&self, scope: ConfigScope, name: &str) -> Result<PathBuf> {
        let dir = self.registries_dir(scope)?;
        Ok(dir.join(format!("{name}.toml")))
    }
}

fn default_global_root() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(Path::new(&home).join(".config").join("warehouse"))
}

fn load_registry_from_path(path: &Path) -> Result<RegistryConfig> {
    let content =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    toml::from_str(&content).with_context(|| format!("failed to parse {}", path.display()))
}
