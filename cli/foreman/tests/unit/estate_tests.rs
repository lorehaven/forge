use foreman::config::Config;
use foreman::estate::*;
use foreman::vars::{self};
use std::path::Path;
use std::path::PathBuf;

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

/// `resolve_skips_env_when_blocks_...` and `resolve_applies_env_when_blocks_...`
/// both set/clear `FOREMAN_ESTATE_TEST_UNSET_MARKER_1a2b3c`, which is
/// process-global - without this, cargo's default parallel test runner
/// can interleave the two and have one test see the other's value.
fn env_when_lock() -> &'static std::sync::Mutex<()> {
    static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(()))
}

#[test]
fn resolve_skips_env_when_blocks_whose_condition_variable_is_not_set() {
    // A variable name specific enough that nothing else could plausibly set it.
    // SAFETY: this test owns the lifetime of this specific, uniquely-named variable.
    let _guard = env_when_lock().lock().unwrap_or_else(|p| p.into_inner());
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
    let _guard = env_when_lock().lock().unwrap_or_else(|p| p.into_inner());
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
