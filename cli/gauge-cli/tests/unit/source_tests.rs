use gauge_cli::config::FileConfig;
use gauge_cli::source::{Auth, Config, Flags};

const fn none(_: &str) -> Option<String> {
    None
}

fn flags(url: Option<&str>, user: Option<&str>, pw: Option<&str>) -> Flags {
    Flags {
        url: url.map(Into::into),
        path: None,
        username: user.map(Into::into),
        password: pw.map(Into::into),
    }
}

fn resolve(
    f: Flags,
    file: &FileConfig,
    env: impl Fn(&str) -> Option<String>,
) -> quench_core::Result<Config> {
    Config::resolve(f, file, None, env)
}

#[quench_test::test]
fn flags_are_enough() {
    let c = resolve(
        flags(Some("https://x.example/"), Some("me"), Some("pw")),
        &FileConfig::default(),
        none,
    )
    .unwrap();
    assert_eq!(c.server, "https://x.example");
    assert_eq!(c.path, "/gauge");
    assert_eq!(
        c.auth,
        Auth::Password {
            username: "me".into(),
            password: "pw".into()
        }
    );
}

#[quench_test::test]
fn the_environment_fills_what_flags_leave_out() {
    let env = |k: &str| match k {
        "GAUGE_URL" => Some("https://e.example".to_string()),
        "GAUGE_USERNAME" => Some("env-user".to_string()),
        "GAUGE_PASSWORD" => Some("env-pw".to_string()),
        "GAUGE_PATH" => Some("metrics/".to_string()),
        _ => None,
    };
    let c = resolve(
        flags(None, Some("flag-user"), None),
        &FileConfig::default(),
        env,
    )
    .unwrap();
    assert_eq!(c.server, "https://e.example");
    assert_eq!(
        c.auth,
        Auth::Password {
            username: "flag-user".into(),
            password: "env-pw".into()
        }
    );
    assert_eq!(c.path, "/metrics");
}

#[quench_test::test]
fn the_file_alone_is_enough() {
    let file = FileConfig {
        url: Some("https://f.example/".into()),
        path: Some("/g".into()),
        token: Some("refresh".into()),
    };
    let c = Config::resolve(
        flags(None, None, None),
        &file,
        Some("/x/config.toml".into()),
        none,
    )
    .unwrap();
    assert_eq!(c.server, "https://f.example");
    assert_eq!(c.path, "/g");
    assert_eq!(c.auth, Auth::Token("refresh".into()));
    assert_eq!(
        c.file.as_deref(),
        Some(std::path::Path::new("/x/config.toml"))
    );
}

#[quench_test::test]
fn environment_and_flags_win_over_the_file() {
    let file = FileConfig {
        url: Some("https://f.example".into()),
        path: None,
        token: Some("file-token".into()),
    };
    let env = |k: &str| (k == "GAUGE_TOKEN").then(|| "env-token".to_string());
    let c = resolve(flags(Some("https://flag.example"), None, None), &file, env).unwrap();
    assert_eq!(c.server, "https://flag.example");
    assert_eq!(c.auth, Auth::Token("env-token".into()));
    let c = resolve(flags(None, Some("me"), Some("pw")), &file, none).unwrap();
    assert!(
        matches!(c.auth, Auth::Password { .. }),
        "a password sign-in beats a stored token"
    );
}

#[quench_test::test]
fn a_missing_setting_says_how_to_give_it() {
    let nothing = FileConfig::default();
    let err = resolve(flags(None, None, None), &nothing, none)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("gauge login") && err.contains("GAUGE_URL"),
        "{err}"
    );
    let err = resolve(flags(Some("u"), None, None), &nothing, none)
        .unwrap_err()
        .to_string();
    assert!(err.contains("gauge login"), "{err}");
    let err = resolve(flags(Some("u"), Some("n"), None), &nothing, none)
        .unwrap_err()
        .to_string();
    assert!(err.contains("GAUGE_PASSWORD"), "{err}");
}

#[quench_test::test]
fn blank_values_count_as_missing() {
    let file = FileConfig {
        url: Some("  ".into()),
        path: None,
        token: Some(" ".into()),
    };
    assert!(resolve(flags(Some("  "), None, None), &file, none).is_err());
    assert!(resolve(flags(Some("u"), None, None), &file, none).is_err());
}

#[quench_test::test]
fn host_names_are_percent_encoded_in_the_path() {
    assert_eq!(gauge_cli::source::host_path("ossiriand"), "hosts/ossiriand");
    assert_eq!(gauge_cli::source::host_path("a b/c"), "hosts/a%20b%2Fc");
}
