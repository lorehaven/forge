use clap::Parser;
use pulley::cli::{Cli, Command, ServiceAction};

#[test]
fn no_subcommand_is_the_default_repl() {
    let cli = Cli::try_parse_from(["pulley"]).unwrap();
    assert!(cli.command.is_none());
}

#[test]
fn daemon_parses_as_its_own_subcommand() {
    let cli = Cli::try_parse_from(["pulley", "daemon"]).unwrap();
    assert!(matches!(cli.command, Some(Command::Daemon)));
}

#[test]
fn service_requires_an_action() {
    let result = Cli::try_parse_from(["pulley", "service"]);
    assert!(result.is_err());
}

#[test]
fn service_install_parses() {
    let cli = Cli::try_parse_from(["pulley", "service", "install"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Service {
            action: ServiceAction::Install
        })
    ));
}

#[test]
fn service_uninstall_parses() {
    let cli = Cli::try_parse_from(["pulley", "service", "uninstall"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Service {
            action: ServiceAction::Uninstall
        })
    ));
}

#[test]
fn service_status_parses() {
    let cli = Cli::try_parse_from(["pulley", "service", "status"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Service {
            action: ServiceAction::Status
        })
    ));
}

#[test]
fn an_unknown_subcommand_is_rejected() {
    let result = Cli::try_parse_from(["pulley", "not-a-real-command"]);
    assert!(result.is_err());
}
