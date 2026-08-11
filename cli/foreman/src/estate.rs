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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn estate(text: &str) -> Estate {
        let config: Config = toml::from_str(text).unwrap();
        let vars = vars::resolve(Path::new("/tmp"), &config.vars).unwrap();
        Estate {
            root: PathBuf::from("/tmp/foreman-estate-test"),
            config_path: PathBuf::from("/tmp/foreman-estate-test/foreman.toml"),
            config,
            vars,
        }
    }

    const CHAIN: &str = r#"
        [[services]]
        name = "db"
        package = "db-svc"
        port = 5432

        [[services]]
        name = "auth"
        package = "auth-svc"
        port = 8080
        needs = ["db"]

        [[services]]
        name = "web"
        package = "web-svc"
        port = 8081
        needs = ["auth"]
    "#;

    #[test]
    fn with_dependencies_pulls_in_the_whole_transitive_chain() {
        let estate = estate(CHAIN);
        let selected = estate.with_dependencies(&["web"]).unwrap();
        assert_eq!(selected, vec!["db", "auth", "web"]);
    }

    #[test]
    fn with_dependencies_dedupes_when_two_selections_share_a_need() {
        let estate = estate(CHAIN);
        let selected = estate.with_dependencies(&["web", "auth"]).unwrap();
        assert_eq!(selected, vec!["db", "auth", "web"]);
    }

    #[test]
    fn with_dependencies_is_always_in_table_order_regardless_of_input_order() {
        let estate = estate(CHAIN);
        let selected = estate.with_dependencies(&["auth", "web"]).unwrap();
        assert_eq!(selected, vec!["db", "auth", "web"]);
    }

    #[test]
    fn resolve_names_empty_means_the_whole_estate_in_table_order() {
        let estate = estate(CHAIN);
        let names: Vec<String> = Vec::new();
        assert_eq!(
            estate.resolve_names(&names).unwrap(),
            vec!["db", "auth", "web"]
        );
    }

    #[test]
    fn resolve_names_rejects_an_unknown_service() {
        let estate = estate(CHAIN);
        assert!(estate.resolve_names(&["nope"]).is_err());
    }

    #[test]
    fn resolve_names_does_not_pull_in_dependencies() {
        let estate = estate(CHAIN);
        assert_eq!(estate.resolve_names(&["web"]).unwrap(), vec!["web"]);
    }

    #[test]
    fn resolve_selection_combines_names_and_dependencies() {
        let estate = estate(CHAIN);
        assert_eq!(
            estate.resolve_selection(&["web"]).unwrap(),
            vec!["db", "auth", "web"]
        );
    }

    #[test]
    fn is_whole_estate_true_only_when_everything_is_selected() {
        let estate = estate(CHAIN);
        assert!(estate.is_whole_estate(&["db".into(), "auth".into(), "web".into()]));
        assert!(!estate.is_whole_estate(&["web".into()]));
    }

    #[test]
    fn reject_unknown_is_ok_for_known_names() {
        let estate = estate(CHAIN);
        assert!(estate.reject_unknown(&["db", "web"]).is_ok());
    }

    #[test]
    fn reject_unknown_reports_a_single_unknown_name_in_singular() {
        let estate = estate(CHAIN);
        let err = estate.reject_unknown(&["nope"]).unwrap_err().to_string();
        assert!(err.contains("unknown service "), "error was: {err}");
        assert!(err.contains("nope"));
    }

    #[test]
    fn reject_unknown_reports_multiple_unknown_names_in_plural() {
        let estate = estate(CHAIN);
        let err = estate
            .reject_unknown(&["nope", "also-nope"])
            .unwrap_err()
            .to_string();
        assert!(err.contains("unknown services "), "error was: {err}");
        assert!(err.contains("nope"));
        assert!(err.contains("also-nope"));
    }

    const WITH_DEFAULTS: &str = r#"
        [defaults]
        scheme = "https"
        host = "localhost"
        health_path = "/health"

        [defaults.env]
        SHARED = "from-defaults"
        SERVER_ADDR = "0.0.0.0:${port}"

        [[services]]
        name = "auth"
        package = "auth-svc"
        port = 8080

        [services.env]
        SHARED = "from-service"
        SERVICE_ONLY = "yes"
    "#;

    #[test]
    fn resolve_folds_defaults_and_lets_the_service_win_on_conflict() {
        let estate = estate(WITH_DEFAULTS);
        let resolved = estate.resolve("auth").unwrap();

        let shared = resolved
            .env
            .iter()
            .find(|(k, _)| k == "SHARED")
            .map(|(_, v)| v.as_str());
        assert_eq!(shared, Some("from-service"));

        let service_only = resolved
            .env
            .iter()
            .find(|(k, _)| k == "SERVICE_ONLY")
            .map(|(_, v)| v.as_str());
        assert_eq!(service_only, Some("yes"));
    }

    #[test]
    fn resolve_expands_the_service_own_fields_into_its_environment_templates() {
        let estate = estate(WITH_DEFAULTS);
        let resolved = estate.resolve("auth").unwrap();
        let addr = resolved
            .env
            .iter()
            .find(|(k, _)| k == "SERVER_ADDR")
            .map(|(_, v)| v.as_str());
        assert_eq!(addr, Some("0.0.0.0:8080"));
    }

    #[test]
    fn resolve_falls_back_through_service_then_defaults_then_the_hardcoded_default() {
        let estate = estate(WITH_DEFAULTS);
        let resolved = estate.resolve("auth").unwrap();
        assert_eq!(resolved.scheme, "https");
        assert_eq!(resolved.host, "localhost");
        assert_eq!(resolved.health_path, "/health");
    }

    const NO_DEFAULTS: &str = r#"
        [[services]]
        name = "auth"
        package = "auth-svc"
        port = 8080
    "#;

    #[test]
    fn resolve_uses_the_hardcoded_fallback_when_nothing_else_is_set() {
        let estate = estate(NO_DEFAULTS);
        let resolved = estate.resolve("auth").unwrap();
        assert_eq!(resolved.scheme, "https");
        assert_eq!(resolved.host, "localhost");
        assert_eq!(resolved.health_path, "/health");
        assert_eq!(resolved.start_timeout_secs, 30);
        assert_eq!(resolved.stop_timeout_secs, 5);
    }

    #[test]
    fn resolve_defaults_the_workdir_to_the_package_name() {
        let estate = estate(NO_DEFAULTS);
        let resolved = estate.resolve("auth").unwrap();
        assert_eq!(resolved.workdir, estate.root.join("auth-svc"));
    }

    const WITH_UNSET: &str = r#"
        [defaults.env]
        SHARED_SECRET = "leaked-if-not-unset"

        [[services]]
        name = "public"
        package = "public-svc"
        port = 9000
        unset = ["SHARED_SECRET"]
    "#;

    #[test]
    fn resolve_drops_unset_keys_even_when_a_default_supplied_them() {
        let estate = estate(WITH_UNSET);
        let resolved = estate.resolve("public").unwrap();
        assert!(!resolved.env.iter().any(|(k, _)| k == "SHARED_SECRET"));
        assert_eq!(resolved.unset, vec!["SHARED_SECRET".to_string()]);
    }

    const WITH_ENV_WHEN: &str = r#"
        [[services]]
        name = "auth"
        package = "auth-svc"
        port = 8080

        [[services.env_when]]
        env_set = "FOREMAN_ESTATE_TEST_UNSET_MARKER_1a2b3c"
        note = "conditional block applied"

        [services.env_when.env]
        EXTRA = "only-when-set"
    "#;

    #[test]
    fn resolve_skips_env_when_blocks_whose_condition_variable_is_not_set() {
        // A variable name specific enough that nothing else could plausibly set it.
        // SAFETY: this test owns the lifetime of this specific, uniquely-named variable.
        unsafe {
            std::env::remove_var("FOREMAN_ESTATE_TEST_UNSET_MARKER_1a2b3c");
        }
        let estate = estate(WITH_ENV_WHEN);
        let resolved = estate.resolve("auth").unwrap();
        assert!(!resolved.env.iter().any(|(k, _)| k == "EXTRA"));
        assert!(resolved.notes.is_empty());
    }

    #[test]
    fn resolve_applies_env_when_blocks_whose_condition_variable_is_set() {
        // SAFETY: this test owns the lifetime of this specific, uniquely-named
        // variable and clears it again before returning.
        unsafe {
            std::env::set_var("FOREMAN_ESTATE_TEST_UNSET_MARKER_1a2b3c", "1");
        }
        let estate = estate(WITH_ENV_WHEN);
        let resolved = estate.resolve("auth").unwrap();
        unsafe {
            std::env::remove_var("FOREMAN_ESTATE_TEST_UNSET_MARKER_1a2b3c");
        }

        let extra = resolved
            .env
            .iter()
            .find(|(k, _)| k == "EXTRA")
            .map(|(_, v)| v.as_str());
        assert_eq!(extra, Some("only-when-set"));
        assert_eq!(
            resolved.notes,
            vec!["conditional block applied".to_string()]
        );
    }

    #[test]
    fn cert_files_falls_back_from_service_to_defaults_to_the_hardcoded_pair() {
        let estate = estate(NO_DEFAULTS);
        let service = estate.service("auth").unwrap();
        assert_eq!(
            estate.cert_files(service),
            vec!["cert.pem".to_string(), "key.pem".to_string()]
        );
    }

    #[test]
    fn cert_files_prefers_the_service_own_list() {
        let text = r#"
            [[services]]
            name = "auth"
            package = "auth-svc"
            port = 8080
            cert_files = ["custom.pem"]
        "#;
        let estate = estate(text);
        let service = estate.service("auth").unwrap();
        assert_eq!(estate.cert_files(service), vec!["custom.pem".to_string()]);
    }

    #[test]
    fn resolved_url_and_health_url_are_assembled_from_the_resolved_fields() {
        let estate = estate(WITH_DEFAULTS);
        let resolved = estate.resolve("auth").unwrap();
        assert_eq!(resolved.url(), "https://localhost:8080");
        assert_eq!(resolved.health_url(), "https://localhost:8080/health");
    }
}
