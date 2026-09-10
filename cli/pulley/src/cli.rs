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
    /// List every configured job (merged from all config files) and exit -
    /// the REPL's `list`, without the REPL.
    List,

    /// Run the named job(s) once and exit - a non-interactive one-shot of
    /// the REPL's `run`, with no REPL prompt and no daemon loop. Pass `all`
    /// to run every configured job. Per-job `no-confirm` still applies.
    Run {
        /// Job id(s) to run, or `all` for every configured job
        #[arg(required = true, value_name = "JOB_ID")]
        jobs: Vec<String>,
    },

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
