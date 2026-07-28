use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "foreman")]
#[command(version)]
#[command(
    about = "Foreman - run a project's local development estate from one TOML file",
    long_about = None
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// A bare service name starts it: `foreman conveyor` is what anyone who has
    /// read the service list types first.
    #[arg(trailing_var_arg = true, hide = true)]
    pub bare: Vec<String>,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Start the estate, or just the services named (plus what they need)
    Start { services: Vec<String> },

    /// Stop the services; `all` reaches the containers underneath them too
    Stop { services: Vec<String> },

    /// What is up, and on which port
    Status,

    /// Follow one service's log
    Logs { service: String },

    /// Pick services interactively, then start them
    #[command(alias = "pick")]
    Repl { services: Vec<String> },

    /// Bring up the database and install the schema catalog, without services
    Db,

    /// Drop what the schema tooling owns and reinstall it (development only)
    Reset,

    /// Run the test suite, or one suite; flags are passed through
    Test {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Run a task from the config by name
    Run {
        task: String,
        /// Services to scope the task to, when it takes a selection
        services: Vec<String>,
    },

    /// List the services, containers and tasks this project defines
    List,

    /// Show the command line and environment one service would be started with
    Env { service: String },

    /// Write a starter foreman.toml in the current directory
    Init {
        /// Overwrite an existing file
        #[arg(long)]
        force: bool,
    },
}
