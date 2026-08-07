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

    /// Manage the systemd --user service that runs `pulley daemon`
    Service {
        #[command(subcommand)]
        action: ServiceAction,
    },
}

#[derive(Subcommand, Debug)]
pub enum ServiceAction {
    /// Write the unit file and enable + start it now
    Install,
    /// Stop, disable and remove the unit file
    Uninstall,
    /// Show `systemctl --user status pulley`
    Status,
}
