use clap::{Args, Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "conveyor",
    version,
    about = "Conveyor CLI - the Forge CI/CD service"
)]
pub struct Cli {
    /// Where conveyor is, e.g. `https://localhost:9443/conveyor`.
    /// Defaults to `$CONVEYOR_URL`.
    #[arg(long, global = true, value_name = "URL")]
    pub url: Option<String>,

    /// Realm account to authenticate as. Defaults to `$CONVEYOR_USERNAME`.
    #[arg(long, global = true, value_name = "NAME")]
    pub username: Option<String>,

    /// Its password. Defaults to `$CONVEYOR_PASSWORD`.
    #[arg(long, global = true, value_name = "PASSWORD")]
    pub password: Option<String>,

    /// Where gatehouse is, e.g. `https://localhost:5443/gatehouse`. Defaults
    /// to `$GATEHOUSE_URL` - required whenever a username/password is given,
    /// since gatehouse is who exchanges them for a token now.
    #[arg(long, global = true, value_name = "URL")]
    pub gatehouse_url: Option<String>,

    /// Accept a self-signed certificate, as the estate's internal ones are.
    #[arg(long, global = true)]
    pub insecure: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Manage the repositories conveyor builds
    Repo {
        #[command(subcommand)]
        command: RepoCommands,
    },
    /// Start a run
    Run(RunArgs),
    /// List recent runs
    Runs(RunsArgs),
    /// Show one run, its jobs and its artifacts
    Show(ShowArgs),
    /// Print a job's output, or follow it as it happens
    Logs(LogsArgs),
    /// Ask a run to stop
    Cancel(CancelArgs),
    /// Manage secrets
    Secret {
        #[command(subcommand)]
        command: SecretCommands,
    },
    /// Check a `.conveyor.toml` without sending it anywhere
    Validate(ValidateArgs),
}

#[derive(Subcommand, Debug)]
pub enum RepoCommands {
    /// Register a repository
    Add(RepoAddArgs),
    /// List registered repositories
    List,
    /// Turn a repository on or off
    Enable(RepoEnableArgs),
    /// Turn a repository off, keeping its history
    Disable(RepoEnableArgs),
    /// Remove a repository and everything it built
    Remove(RepoRefArgs),
}

#[derive(Args, Debug)]
pub struct RepoAddArgs {
    /// `owner/name`, as the provider knows it
    pub slug: String,
    /// Where to clone from
    pub clone_url: String,
    /// `github` or `generic`
    #[arg(long, default_value = "github")]
    pub provider: String,
    #[arg(long, default_value = "master")]
    pub default_branch: String,
}

#[derive(Args, Debug)]
pub struct RepoRefArgs {
    /// `owner/name`, or the repository's id
    pub repo: String,
}

#[derive(Args, Debug)]
pub struct RepoEnableArgs {
    /// `owner/name`, or the repository's id
    pub repo: String,
}

#[derive(Args, Debug)]
pub struct RunArgs {
    /// `owner/name`, or the repository's id
    pub repo: String,
    /// Branch or tag to build. Defaults to the repository's default branch.
    #[arg(long, value_name = "REF")]
    pub git_ref: Option<String>,
    /// The commit to build. Without it conveyor asks the repository what the
    /// ref currently points at.
    #[arg(long)]
    pub sha: Option<String>,
    /// Wait for the run to finish, and exit non-zero if it fails
    #[arg(long)]
    pub wait: bool,
}

#[derive(Args, Debug)]
pub struct RunsArgs {
    /// Only this repository's runs
    #[arg(long, value_name = "REPO")]
    pub repo: Option<String>,
    #[arg(long, default_value_t = 20)]
    pub limit: i64,
}

#[derive(Args, Debug)]
pub struct ShowArgs {
    pub run_id: String,
}

#[derive(Args, Debug)]
pub struct LogsArgs {
    /// A run id (its jobs are printed in order) or a single job id
    pub id: String,
    /// Follow the output as it happens
    #[arg(long, short)]
    pub follow: bool,
}

#[derive(Args, Debug)]
pub struct CancelArgs {
    pub run_id: String,
}

#[derive(Subcommand, Debug)]
pub enum SecretCommands {
    /// Write a secret, replacing whatever was there
    Set(SecretSetArgs),
    /// List secret names. Values are never returned.
    List(SecretScopeArgs),
    /// Remove a secret
    Remove(SecretRemoveArgs),
}

#[derive(Args, Debug)]
pub struct SecretSetArgs {
    pub name: String,
    /// The value. Omit to read it from stdin, which keeps it out of shell
    /// history and out of the process list.
    pub value: Option<String>,
    /// Scope it to one repository rather than the whole estate
    #[arg(long, value_name = "REPO")]
    pub repo: Option<String>,
}

#[derive(Args, Debug)]
pub struct SecretScopeArgs {
    #[arg(long, value_name = "REPO")]
    pub repo: Option<String>,
}

#[derive(Args, Debug)]
pub struct SecretRemoveArgs {
    pub name: String,
    #[arg(long, value_name = "REPO")]
    pub repo: Option<String>,
}

#[derive(Args, Debug)]
pub struct ValidateArgs {
    /// The file to check. Defaults to `.conveyor.toml` here.
    #[arg(default_value = ".conveyor.toml")]
    pub path: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn repo_add_defaults_provider_and_branch() {
        let cli = Cli::try_parse_from([
            "conveyor",
            "repo",
            "add",
            "owner/name",
            "https://example.com/owner/name.git",
        ])
        .unwrap();
        let Commands::Repo {
            command: RepoCommands::Add(args),
        } = cli.command
        else {
            panic!("expected RepoCommands::Add");
        };
        assert_eq!(args.provider, "github");
        assert_eq!(args.default_branch, "master");
    }

    #[test]
    fn runs_defaults_limit_to_twenty() {
        let cli = Cli::try_parse_from(["conveyor", "runs"]).unwrap();
        let Commands::Runs(args) = cli.command else {
            panic!("expected Commands::Runs");
        };
        assert_eq!(args.limit, 20);
        assert_eq!(args.repo, None);
    }

    #[test]
    fn validate_defaults_path_to_conveyor_toml() {
        let cli = Cli::try_parse_from(["conveyor", "validate"]).unwrap();
        let Commands::Validate(args) = cli.command else {
            panic!("expected Commands::Validate");
        };
        assert_eq!(args.path, ".conveyor.toml");
    }

    #[test]
    fn global_flags_are_readable_after_a_subcommand_is_parsed() {
        let cli = Cli::try_parse_from([
            "conveyor",
            "--url",
            "https://localhost:9443/conveyor",
            "--insecure",
            "repo",
            "list",
        ])
        .unwrap();
        assert_eq!(cli.url.as_deref(), Some("https://localhost:9443/conveyor"));
        assert!(cli.insecure);
    }

    #[test]
    fn run_requires_a_repo_argument() {
        let result = Cli::try_parse_from(["conveyor", "run"]);
        assert!(result.is_err());
    }

    #[test]
    fn secret_set_value_is_optional_so_it_can_be_read_from_stdin() {
        let cli = Cli::try_parse_from(["conveyor", "secret", "set", "API_KEY"]).unwrap();
        let Commands::Secret {
            command: SecretCommands::Set(args),
        } = cli.command
        else {
            panic!("expected SecretCommands::Set");
        };
        assert_eq!(args.name, "API_KEY");
        assert_eq!(args.value, None);
    }
}
