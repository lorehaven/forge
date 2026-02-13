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
        } => {
            assert!(!all);
            assert_eq!(package.as_deref(), Some("ferrous"));
            assert_eq!(test_name.as_deref(), Some("ui_web"));
            assert!(ignored);
            assert!(!list);
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
        } => {
            assert!(!all);
            assert_eq!(package.as_deref(), Some("ferrous"));
            assert!(test_name.is_none());
            assert!(!ignored);
            assert!(list);
        }
        _ => panic!("expected test command"),
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
