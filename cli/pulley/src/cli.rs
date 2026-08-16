use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "pulley")]
#[command(version)]
#[command(
    about = "Pulley - rsync-backed backup/sync jobs, interactively or as a background service",
    long_about = None
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Run continuous sync in the foreground, polling every job's `interval`
    Daemon,

    /// Manage the background service that runs `pulley daemon`
    /// (systemd --user or runit, auto-detected, on Linux; a logon
    /// Scheduled Task on Windows)
    Service {
        #[command(subcommand)]
        action: ServiceAction,
    },
}

#[derive(Subcommand, Debug)]
pub enum ServiceAction {
    /// Write the service definition and enable + start it now
    Install,
    /// Stop, disable and remove the service definition
    Uninstall,
    /// Show the service's status
    Status,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

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
}
