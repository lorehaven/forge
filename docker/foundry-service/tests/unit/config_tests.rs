use foundry_service::config::{ConfigInputs, FileConfig, FileInstall, RunConfig, env_value, pick};
use quench_db::prelude::InstallRequest;
use semver::Version;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;

/// `RunConfig::resolve` reads a fixed set of env vars, so tests that set
/// any of them must not interleave with each other under the default
/// multi-threaded test runner.
fn env_lock() -> &'static Mutex<()> {
    static LOCK: std::sync::OnceLock<Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

const ENV_KEYS: &[&str] = &[
    "FOUNDRY_CONFIG",
    "FOUNDRY_CATALOG",
    "DATABASE_URL",
    "POSTGRES_URL",
    "FOUNDRY_INSTALL",
    "FOUNDRY_LEDGER_SCHEMA",
    "FOUNDRY_LEDGER_TABLE",
    "FOUNDRY_MODULE_TABLE",
];

/// Clears every env var `resolve` reads, and restores that on drop -
/// so one test's `set_var` calls can't leak into the next.
struct EnvGuard<'a> {
    _held: std::sync::MutexGuard<'a, ()>,
    saved: Vec<(&'static str, Option<String>)>,
}

impl<'a> EnvGuard<'a> {
    fn new(held: std::sync::MutexGuard<'a, ()>) -> Self {
        let saved = ENV_KEYS
            .iter()
            .map(|&key| (key, std::env::var(key).ok()))
            .collect();
        for &key in ENV_KEYS {
            unsafe { std::env::remove_var(key) };
        }
        Self { _held: held, saved }
    }

    fn set(&self, key: &str, value: &str) {
        unsafe { std::env::set_var(key, value) };
    }
}

impl Drop for EnvGuard<'_> {
    fn drop(&mut self) {
        for (key, value) in &self.saved {
            match value {
                Some(value) => unsafe { std::env::set_var(key, value) },
                None => unsafe { std::env::remove_var(key) },
            }
        }
    }
}

fn clean_env() -> EnvGuard<'static> {
    EnvGuard::new(
        env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()),
    )
}

fn empty_inputs<'a>(installs: &'a [String]) -> ConfigInputs<'a> {
    ConfigInputs {
        config_path: None,
        catalog: None,
        database_url: None,
        installs,
        ledger_schema: None,
        ledger_table: None,
        module_table: None,
        database_optional: true,
    }
}

#[test]
fn file_config_load_parses_installs() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("install.toml");
    std::fs::write(
        &path,
        r#"
                database_url = "postgres://file/db"
                ledger_schema = "file_schema"

                [[install]]
                module = "gatehouse"
                version = "1.2.3"
                schema = "auth"

                [install.variables]
                foo = "bar"
            "#,
    )
    .expect("write config");

    let config = FileConfig::load(&path).expect("load");
    assert_eq!(config.database_url.as_deref(), Some("postgres://file/db"));
    assert_eq!(config.ledger_schema.as_deref(), Some("file_schema"));
    assert_eq!(config.install.len(), 1);
    assert_eq!(config.install[0].module, "gatehouse");
    assert_eq!(
        config.install[0].variables.get("foo").map(String::as_str),
        Some("bar")
    );
}

#[test]
fn file_config_load_missing_file_errors() {
    let error = FileConfig::load(Path::new("/does/not/exist/install.toml")).unwrap_err();
    assert!(error.to_string().contains("failed to read config"));
}

#[test]
fn file_config_load_rejects_unknown_fields() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("install.toml");
    std::fs::write(&path, "unexpected_field = true").expect("write config");

    let error = FileConfig::load(&path).unwrap_err();
    assert!(error.to_string().contains("invalid config"));
}

#[test]
fn file_install_into_install_request_maps_every_field() {
    let mut variables = BTreeMap::new();
    variables.insert("key".to_string(), "value".to_string());
    let install = FileInstall {
        module: "sage".to_string(),
        version: Some(Version::parse("2.0.0").expect("version")),
        schema: Some("sage_schema".to_string()),
        variables: variables.clone(),
    };

    let request: InstallRequest = install.into();
    assert_eq!(request.module, "sage");
    assert_eq!(
        request.version.map(|v| v.to_string()),
        Some("2.0.0".to_string())
    );
    assert_eq!(request.schema.as_deref(), Some("sage_schema"));
    assert_eq!(request.variables, variables);
}

#[test]
fn resolve_requires_database_url_unless_optional() {
    let _guard = clean_env();
    let installs = vec!["gatehouse".to_string()];
    let mut inputs = empty_inputs(&installs);
    inputs.database_optional = false;

    let error = RunConfig::resolve(inputs).unwrap_err();
    assert!(error.to_string().contains("no database configured"));
}

#[test]
fn resolve_allows_missing_database_url_when_optional() {
    let _guard = clean_env();
    let installs = vec!["gatehouse".to_string()];
    let inputs = empty_inputs(&installs);

    let config = RunConfig::resolve(inputs).expect("resolve");
    assert_eq!(config.database_url, "");
}

#[test]
fn resolve_rejects_empty_install_list() {
    let _guard = clean_env();
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("empty.toml");
    std::fs::write(&path, "").expect("write config");

    let installs: Vec<String> = Vec::new();
    let mut inputs = empty_inputs(&installs);
    inputs.config_path = Some(&path);

    let error = RunConfig::resolve(inputs).unwrap_err();
    assert!(error.to_string().contains("nothing to install"));
}

#[test]
fn resolve_cli_flag_beats_env_for_database_url() {
    let guard = clean_env();
    guard.set("DATABASE_URL", "postgres://env/db");
    let installs = vec!["gatehouse".to_string()];
    let mut inputs = empty_inputs(&installs);
    inputs.database_url = Some("postgres://cli/db");

    let config = RunConfig::resolve(inputs).expect("resolve");
    assert_eq!(config.database_url, "postgres://cli/db");
}

#[test]
fn resolve_falls_back_to_postgres_url_env() {
    let guard = clean_env();
    guard.set("POSTGRES_URL", "postgres://postgres-env/db");
    let installs = vec!["gatehouse".to_string()];
    let inputs = empty_inputs(&installs);

    let config = RunConfig::resolve(inputs).expect("resolve");
    assert_eq!(config.database_url, "postgres://postgres-env/db");
}

#[test]
fn resolve_parses_explicit_install_specs() {
    let _guard = clean_env();
    let installs = vec!["gatehouse@1.0.0:auth".to_string()];
    let mut inputs = empty_inputs(&installs);
    inputs.database_optional = true;

    let config = RunConfig::resolve(inputs).expect("resolve");
    assert_eq!(config.installs.len(), 1);
    assert_eq!(config.installs[0].module, "gatehouse");
}

#[test]
fn resolve_reads_installs_from_env_when_none_passed() {
    let guard = clean_env();
    guard.set("FOUNDRY_INSTALL", " gatehouse , , sage ");
    let installs: Vec<String> = Vec::new();
    let inputs = empty_inputs(&installs);

    let config = RunConfig::resolve(inputs).expect("resolve");
    let modules: Vec<&str> = config.installs.iter().map(|i| i.module.as_str()).collect();
    assert_eq!(modules, vec!["gatehouse", "sage"]);
}

#[test]
fn resolve_uses_config_file_installs_when_nothing_else_given() {
    let guard = clean_env();
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("install.toml");
    std::fs::write(
        &path,
        r#"
                [[install]]
                module = "warehouse"
            "#,
    )
    .expect("write config");
    guard.set("FOUNDRY_CONFIG", path.to_str().expect("utf8 path"));

    let installs: Vec<String> = Vec::new();
    let inputs = empty_inputs(&installs);

    let config = RunConfig::resolve(inputs).expect("resolve");
    assert_eq!(config.installs.len(), 1);
    assert_eq!(config.installs[0].module, "warehouse");
}

#[test]
fn resolve_errors_when_config_path_does_not_exist() {
    let _guard = clean_env();
    let installs = vec!["gatehouse".to_string()];
    let mut inputs = empty_inputs(&installs);
    let missing = Path::new("/definitely/missing/install.toml");
    inputs.config_path = Some(missing);

    let error = RunConfig::resolve(inputs).unwrap_err();
    assert!(error.to_string().contains("does not exist"));
}

#[test]
fn resolve_defaults_ledger_names_when_nothing_overrides_them() {
    let _guard = clean_env();
    let installs = vec!["gatehouse".to_string()];
    let inputs = empty_inputs(&installs);

    let config = RunConfig::resolve(inputs).expect("resolve");
    assert_eq!(
        config.ledger_schema,
        quench_db::runner::DEFAULT_LEDGER_SCHEMA
    );
    assert_eq!(config.ledger_table, quench_db::runner::DEFAULT_LEDGER_TABLE);
    assert_eq!(config.module_table, quench_db::runner::DEFAULT_MODULE_TABLE);
}

#[test]
fn resolve_cli_flag_beats_env_for_ledger_schema() {
    let guard = clean_env();
    guard.set("FOUNDRY_LEDGER_SCHEMA", "env_schema");
    let installs = vec!["gatehouse".to_string()];
    let mut inputs = empty_inputs(&installs);
    inputs.ledger_schema = Some("cli_schema");

    let config = RunConfig::resolve(inputs).expect("resolve");
    assert_eq!(config.ledger_schema, "cli_schema");
}

#[test]
fn pick_prefers_flag_then_env_then_file_then_default() {
    let guard = clean_env();
    assert_eq!(
        pick(
            Some("flag"),
            "FOUNDRY_LEDGER_SCHEMA",
            Some("file".to_string()),
            "default"
        ),
        "flag"
    );

    guard.set("FOUNDRY_LEDGER_SCHEMA", "env");
    assert_eq!(
        pick(
            None,
            "FOUNDRY_LEDGER_SCHEMA",
            Some("file".to_string()),
            "default"
        ),
        "env"
    );

    unsafe { std::env::remove_var("FOUNDRY_LEDGER_SCHEMA") };
    assert_eq!(
        pick(
            None,
            "FOUNDRY_LEDGER_SCHEMA",
            Some("file".to_string()),
            "default"
        ),
        "file"
    );
    assert_eq!(
        pick(None, "FOUNDRY_LEDGER_SCHEMA", None, "default"),
        "default"
    );
}

#[test]
fn env_value_treats_empty_string_as_absent() {
    let guard = clean_env();
    guard.set("FOUNDRY_CATALOG", "");
    assert_eq!(env_value("FOUNDRY_CATALOG"), None);

    guard.set("FOUNDRY_CATALOG", "migrations");
    assert_eq!(env_value("FOUNDRY_CATALOG"), Some("migrations".to_string()));
}
