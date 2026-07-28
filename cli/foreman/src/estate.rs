//! The loaded project: its config, its resolved variables, and the questions
//! everything else asks of them.

use anyhow::{Context, Result, bail};
use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::config::{Config, Service};
use crate::vars::{self, Scope};

pub struct Estate {
    pub root: PathBuf,
    pub config_path: PathBuf,
    pub config: Config,
    pub vars: BTreeMap<String, String>,
}

/// A service with the defaults folded in, so callers never have to remember
/// which settings fall back to what.
pub struct Resolved {
    pub name: String,
    pub package: String,
    pub port: u16,
    pub base_path: String,
    pub scheme: String,
    pub host: String,
    pub health_path: String,
    pub start_timeout_secs: u64,
    pub stop_timeout_secs: u64,
    pub workdir: PathBuf,
    pub binary: PathBuf,
    pub build: Vec<String>,
    /// Ready to hand to the child, shared environment first and the service's
    /// own last, so a service overrides the estate rather than the reverse.
    pub env: Vec<(String, String)>,
    pub unset: Vec<String>,
    /// Printed when a conditional environment block applied.
    pub notes: Vec<String>,
}

impl Resolved {
    pub fn url(&self) -> String {
        format!(
            "{}://{}:{}{}",
            self.scheme, self.host, self.port, self.base_path
        )
    }

    pub fn health_url(&self) -> String {
        format!("{}{}", self.url(), self.health_path)
    }
}

impl Estate {
    pub fn load() -> Result<Self> {
        let cwd = std::env::current_dir().context("reading the working directory")?;
        let (root, config_path) = Config::discover(&cwd)?;
        let config = Config::parse(&config_path)?;
        let mut vars = vars::resolve(&root, &config.vars)?;

        // Built-ins. Services run from their own working directory, so a path
        // handed to one has to be absolute; `${project_root}` is how a config
        // writes one without hardcoding where the checkout lives. A config that
        // defines these itself keeps its own value.
        vars.entry("project_root".to_string())
            .or_insert_with(|| root.display().to_string());
        vars.entry("project_name".to_string())
            .or_insert_with(|| config.project.name.clone());

        Ok(Self {
            root,
            config_path,
            config,
            vars,
        })
    }

    pub fn scope(&self) -> Scope<'_> {
        Scope::new(&self.vars)
    }

    pub fn run_dir(&self) -> PathBuf {
        self.root.join(&self.config.project.run_dir)
    }

    pub fn log_dir(&self) -> PathBuf {
        self.run_dir().join("logs")
    }

    pub fn pid_file(&self, name: &str) -> PathBuf {
        self.run_dir().join(format!("{name}.pid"))
    }

    pub fn log_file(&self, name: &str) -> PathBuf {
        self.log_dir().join(format!("{name}.log"))
    }

    pub fn ensure_dirs(&self) -> Result<()> {
        std::fs::create_dir_all(self.log_dir())
            .with_context(|| format!("creating {}", self.log_dir().display()))
    }

    pub fn service_names(&self) -> Vec<String> {
        self.config
            .services
            .iter()
            .map(|s| s.name.clone())
            .collect()
    }

    pub fn service(&self, name: &str) -> Result<&Service> {
        self.config.service(name).with_context(|| {
            format!(
                "unknown service '{name}' (known: {})",
                self.service_names().join(", ")
            )
        })
    }

    /// Puts a set of names in the order the config file lists them. Everything
    /// downstream iterates the table rather than the argument list, so a
    /// selection is always in start order however it was typed or clicked.
    pub fn in_table_order<S: AsRef<str>>(&self, names: &[S]) -> Vec<String> {
        self.config
            .services
            .iter()
            .filter(|service| names.iter().any(|n| n.as_ref() == service.name))
            .map(|service| service.name.clone())
            .collect()
    }

    /// A service is no use without the ones it authenticates against, so asking
    /// for one and getting a version of it that fails every request would be
    /// the wrong kind of obedience. Pulling the dependencies in is quiet: they
    /// are printed as part of the selection, so what actually started is never
    /// a surprise.
    pub fn with_dependencies<S: AsRef<str>>(&self, names: &[S]) -> Result<Vec<String>> {
        let mut wanted: Vec<String> = Vec::new();
        let mut pending: Vec<String> = names.iter().map(|n| n.as_ref().to_string()).collect();

        while let Some(name) = pending.pop() {
            if wanted.contains(&name) {
                continue;
            }
            let service = self.service(&name)?;
            pending.extend(service.needs.iter().cloned());
            wanted.push(name);
        }

        Ok(self.in_table_order(&wanted))
    }

    /// Validates whatever was on the command line and puts it in table order.
    /// No names means the whole estate.
    pub fn resolve_names<S: AsRef<str>>(&self, names: &[S]) -> Result<Vec<String>> {
        if names.is_empty() {
            return Ok(self.service_names());
        }
        for name in names {
            self.service(name.as_ref())?;
        }
        Ok(self.in_table_order(names))
    }

    /// What to start: the names asked for, plus what they cannot start without.
    pub fn resolve_selection<S: AsRef<str>>(&self, names: &[S]) -> Result<Vec<String>> {
        let named = self.resolve_names(names)?;
        self.with_dependencies(&named)
    }

    pub fn is_whole_estate(&self, selection: &[String]) -> bool {
        selection.len() >= self.config.services.len()
    }

    /// Folds the defaults into one service and expands every template in it.
    pub fn resolve(&self, name: &str) -> Result<Resolved> {
        let service = self.service(name)?;
        let defaults = &self.config.defaults;

        // The service lends its own fields to every template in its block,
        // which is what lets the shared environment say `SERVER_ADDR =
        // "0.0.0.0:${port}"` once instead of once per service.
        let scope = self
            .scope()
            .with("name", &service.name)
            .with("package", &service.package)
            .with("port", service.port.to_string())
            .with("base_path", &service.base_path)
            .with("service", &service.name);

        let workdir = service
            .workdir
            .as_deref()
            .or(defaults.workdir.as_deref())
            .unwrap_or("${package}");
        let binary = service
            .binary
            .as_deref()
            .unwrap_or(&self.config.build.binary);
        let build = service.build.as_ref().unwrap_or(&self.config.build.command);

        let mut env = Vec::new();
        let mut seen: BTreeMap<String, usize> = BTreeMap::new();
        let mut notes = Vec::new();

        let mut push =
            |key: String, value: String, env: &mut Vec<(String, String)>| match seen.get(&key) {
                Some(index) => env[*index] = (key, value),
                None => {
                    seen.insert(key.clone(), env.len());
                    env.push((key, value));
                }
            };

        for (key, value) in scope.expand_map(&defaults.env)? {
            push(key, value, &mut env);
        }
        for (key, value) in scope.expand_map(&service.env)? {
            push(key, value, &mut env);
        }
        for conditional in &service.env_when {
            if std::env::var_os(&conditional.env_set).is_none() {
                continue;
            }
            if let Some(note) = &conditional.note {
                notes.push(scope.expand(note)?);
            }
            for (key, value) in scope.expand_map(&conditional.env)? {
                push(key, value, &mut env);
            }
        }

        // `unset` has to reach the merged environment, not just the inherited
        // one: its whole purpose is keeping a shared default off one service,
        // and a shared default is something this map already holds.
        let unset = scope.expand_all(&service.unset)?;
        env.retain(|(key, _)| !unset.contains(key));

        Ok(Resolved {
            name: service.name.clone(),
            package: service.package.clone(),
            port: service.port,
            base_path: service.base_path.clone(),
            scheme: pick(
                &scope,
                service.scheme.as_deref(),
                defaults.scheme.as_deref(),
                "https",
            )?,
            host: pick(
                &scope,
                service.host.as_deref(),
                defaults.host.as_deref(),
                "localhost",
            )?,
            health_path: pick(
                &scope,
                service.health_path.as_deref(),
                defaults.health_path.as_deref(),
                "/health",
            )?,
            start_timeout_secs: service
                .start_timeout_secs
                .or(defaults.start_timeout_secs)
                .unwrap_or(30),
            stop_timeout_secs: service
                .stop_timeout_secs
                .or(defaults.stop_timeout_secs)
                .unwrap_or(5),
            workdir: self.root.join(scope.expand(workdir)?),
            binary: self.root.join(scope.expand(binary)?),
            build: scope.expand_all(build)?,
            env,
            unset,
            notes,
        })
    }

    /// Certificate files a service borrows, if it is configured to borrow any.
    pub fn cert_files(&self, service: &Service) -> Vec<String> {
        if !service.cert_files.is_empty() {
            return service.cert_files.clone();
        }
        if !self.config.defaults.cert_files.is_empty() {
            return self.config.defaults.cert_files.clone();
        }
        vec!["cert.pem".to_string(), "key.pem".to_string()]
    }

    pub fn path(&self, relative: &str) -> PathBuf {
        self.root.join(relative)
    }
}

fn pick(
    scope: &Scope,
    service: Option<&str>,
    default: Option<&str>,
    fallback: &str,
) -> Result<String> {
    scope.expand(service.or(default).unwrap_or(fallback))
}

impl Estate {
    /// Names the caller typed that are not services, reported together rather
    /// than one failed run at a time.
    pub fn reject_unknown<S: AsRef<str>>(&self, names: &[S]) -> Result<()> {
        let known = self.service_names();
        let unknown: Vec<&str> = names
            .iter()
            .map(AsRef::as_ref)
            .filter(|name| !known.iter().any(|k| k == name))
            .collect();

        if !unknown.is_empty() {
            bail!(
                "unknown service{} {} (known: {})",
                if unknown.len() == 1 { "" } else { "s" },
                unknown.join(", "),
                known.join(", ")
            );
        }
        Ok(())
    }
}
