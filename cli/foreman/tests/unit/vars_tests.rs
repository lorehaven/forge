use foreman::vars::*;
use std::collections::BTreeMap;

fn globals() -> BTreeMap<String, String> {
    BTreeMap::from([("host".to_string(), "localhost".to_string())])
}

#[test]
fn expands_globals_and_locals() {
    let globals = globals();
    let scope = Scope::new(&globals).with("port", "8443");
    assert_eq!(
        scope.expand("https://${host}:${port}/health").unwrap(),
        "https://localhost:8443/health"
    );
}

#[test]
fn locals_win_over_globals() {
    let globals = globals();
    let scope = Scope::new(&globals).with("host", "127.0.0.1");
    assert_eq!(scope.expand("${host}").unwrap(), "127.0.0.1");
}

#[test]
fn a_lone_dollar_is_literal() {
    let globals = globals();
    let scope = Scope::new(&globals);
    assert_eq!(
        scope.expand("costs $5, not ${host}").unwrap(),
        "costs $5, not localhost"
    );
}

#[test]
fn unknown_names_are_an_error() {
    let globals = globals();
    let scope = Scope::new(&globals);
    assert!(
        scope
            .expand("${nope}")
            .unwrap_err()
            .to_string()
            .contains("nope")
    );
}

#[test]
fn unterminated_braces_are_an_error() {
    let globals = globals();
    assert!(Scope::new(&globals).expand("${host").is_err());
}

#[test]
fn reads_a_quoted_dotenv_value() {
    let dir = std::env::temp_dir().join("foreman-vars-test");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join(".env");
    std::fs::write(&file, "OTHER=1\nJWT_SECRET=\"sh h\"\nJWT_SECRET=later\n").unwrap();

    assert_eq!(
        env_file_value(&file, "JWT_SECRET").unwrap().as_deref(),
        Some("sh h")
    );
    assert_eq!(env_file_value(&file, "MISSING").unwrap(), None);
    std::fs::remove_dir_all(&dir).ok();
}
