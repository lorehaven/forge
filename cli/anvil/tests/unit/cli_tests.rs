use anvil::cli::{Cli, Commands};
use clap::Parser;

#[test]
fn parse_run_defaults_to_build_and_run_mode() {
    let cli = Cli::parse_from(["anvil", "run"]);
    match cli.command {
        Commands::Run {
            package,
            serve,
            watch_interval_ms,
        } => {
            assert!(package.is_none());
            assert!(!serve);
            assert_eq!(watch_interval_ms, 1000);
        }
        _ => panic!("expected run command"),
    }
}

#[test]
fn parse_run_supports_package_and_serve_mode() {
    let cli = Cli::parse_from([
        "anvil",
        "run",
        "--package",
        "ferrous",
        "--serve",
        "--watch-interval-ms",
        "1500",
    ]);
    match cli.command {
        Commands::Run {
            package,
            serve,
            watch_interval_ms,
        } => {
            assert_eq!(package.as_deref(), Some("ferrous"));
            assert!(serve);
            assert_eq!(watch_interval_ms, 1500);
        }
        _ => panic!("expected run command"),
    }
}

#[test]
fn parse_test_supports_package_name_and_ignored() {
    let cli = Cli::parse_from([
        "anvil",
        "test",
        "--package",
        "ferrous",
        "ui_web",
        "--ignored",
    ]);
    match cli.command {
        Commands::Test {
            all,
            package,
            test_name,
            ignored,
            list,
            json,
        } => {
            assert!(!all);
            assert_eq!(package.as_deref(), Some("ferrous"));
            assert_eq!(test_name.as_deref(), Some("ui_web"));
            assert!(ignored);
            assert!(!list);
            assert!(!json);
        }
        _ => panic!("expected test command"),
    }
}

#[test]
fn parse_test_supports_list_and_package_filter() {
    let cli = Cli::parse_from(["anvil", "test", "--package", "ferrous", "--list"]);
    match cli.command {
        Commands::Test {
            all,
            package,
            test_name,
            ignored,
            list,
            json,
        } => {
            assert!(!all);
            assert_eq!(package.as_deref(), Some("ferrous"));
            assert!(test_name.is_none());
            assert!(!ignored);
            assert!(list);
            assert!(!json);
        }
        _ => panic!("expected test command"),
    }
}

#[test]
fn parse_test_supports_json_flag() {
    let cli = Cli::parse_from(["anvil", "test", "--json"]);
    match cli.command {
        Commands::Test { json, .. } => assert!(json),
        _ => panic!("expected test command"),
    }
}

#[test]
fn parse_nextest_supports_package_name_and_ignored() {
    let cli = Cli::parse_from([
        "anvil",
        "nextest",
        "--package",
        "ferrous",
        "ui_web",
        "--ignored",
    ]);
    match cli.command {
        Commands::Nextest {
            all,
            package,
            test_name,
            ignored,
            json,
        } => {
            assert!(!all);
            assert_eq!(package.as_deref(), Some("ferrous"));
            assert_eq!(test_name.as_deref(), Some("ui_web"));
            assert!(ignored);
            assert!(!json);
        }
        _ => panic!("expected nextest command"),
    }
}

#[test]
fn parse_nextest_supports_json_flag() {
    let cli = Cli::parse_from(["anvil", "nextest", "--json"]);
    match cli.command {
        Commands::Nextest { json, .. } => assert!(json),
        _ => panic!("expected nextest command"),
    }
}

#[test]
fn parse_deny_takes_no_positional_arguments() {
    let cli = Cli::parse_from(["anvil", "deny"]);
    match cli.command {
        Commands::Deny { json } => assert!(!json),
        _ => panic!("expected deny command"),
    }
}

#[test]
fn parse_deny_supports_json_flag() {
    let cli = Cli::parse_from(["anvil", "deny", "--json"]);
    match cli.command {
        Commands::Deny { json } => assert!(json),
        _ => panic!("expected deny command"),
    }
}

#[test]
fn parse_audit_supports_json_flag() {
    let cli = Cli::parse_from(["anvil", "audit", "--json"]);
    match cli.command {
        Commands::Audit { json } => assert!(json),
        _ => panic!("expected audit command"),
    }
}

#[test]
fn parse_machete_supports_json_flag() {
    let cli = Cli::parse_from(["anvil", "machete", "--json"]);
    match cli.command {
        Commands::Machete { json } => assert!(json),
        _ => panic!("expected machete command"),
    }
}

#[test]
fn parse_lint_supports_json_flag() {
    let cli = Cli::parse_from(["anvil", "lint", "--json"]);
    match cli.command {
        Commands::Lint { json, .. } => assert!(json),
        _ => panic!("expected lint command"),
    }
}


#[test]
fn parse_semver_check_supports_package_and_baseline_rev() {
    let cli = Cli::parse_from([
        "anvil",
        "semver-check",
        "--package",
        "conveyor-pipeline",
        "--baseline-rev",
        "d69e26d",
    ]);
    match cli.command {
        Commands::SemverCheck {
            package,
            baseline_rev,
        } => {
            assert_eq!(package, "conveyor-pipeline");
            assert_eq!(baseline_rev.as_deref(), Some("d69e26d"));
        }
        _ => panic!("expected semver-check command"),
    }
}

#[test]
fn parse_semver_check_baseline_rev_is_optional() {
    let cli = Cli::parse_from(["anvil", "semver-check", "--package", "conveyor-pipeline"]);
    match cli.command {
        Commands::SemverCheck {
            package,
            baseline_rev,
        } => {
            assert_eq!(package, "conveyor-pipeline");
            assert!(baseline_rev.is_none());
        }
        _ => panic!("expected semver-check command"),
    }
}

#[test]
fn parse_install_supports_package_flag() {
    let cli = Cli::parse_from(["anvil", "install", "--package", "ferrous"]);
    match cli.command {
        Commands::Install { all, package } => {
            assert!(!all);
            assert_eq!(package.as_deref(), Some("ferrous"));
        }
        _ => panic!("expected install command"),
    }
}

#[test]
fn parse_install_supports_all_flag() {
    let cli = Cli::parse_from(["anvil", "install", "--all"]);
    match cli.command {
        Commands::Install { all, package } => {
            assert!(all);
            assert!(package.is_none());
        }
        _ => panic!("expected install command"),
    }
}

#[test]
fn parse_release_supports_package_flag() {
    let cli = Cli::parse_from(["anvil", "release", "--package", "ferrous"]);
    match cli.command {
        Commands::Release {
            all,
            package,
            dry_run,
        } => {
            assert!(!all);
            assert_eq!(package.as_deref(), Some("ferrous"));
            assert!(!dry_run);
        }
        _ => panic!("expected release command"),
    }
}

#[test]
fn parse_release_supports_all_flag() {
    let cli = Cli::parse_from(["anvil", "release", "--all"]);
    match cli.command {
        Commands::Release {
            all,
            package,
            dry_run,
        } => {
            assert!(all);
            assert!(package.is_none());
            assert!(!dry_run);
        }
        _ => panic!("expected release command"),
    }
}

#[test]
fn parse_release_supports_dry_run_flag() {
    let cli = Cli::parse_from(["anvil", "release", "--all", "--dry-run"]);
    match cli.command {
        Commands::Release {
            all,
            package,
            dry_run,
        } => {
            assert!(all);
            assert!(package.is_none());
            assert!(dry_run);
        }
        _ => panic!("expected release command"),
    }
}
