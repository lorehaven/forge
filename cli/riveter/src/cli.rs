use clap::{Parser, Subcommand, ValueEnum};

const TARGET_HELP: &str = "Resources to act on as kind[/name], e.g. `deployment/api`, \
                           `statefulset` or `*/api`. Both halves accept `*` and `?` \
                           wildcards. Omit to act on every resource in scope.";

/// Replaces clap's flat subcommand list with the same command tree the REPL's
/// `help` prints, so both surfaces describe riveter identically.
#[must_use]
pub fn help_template() -> String {
    format!(
        "{{usage-heading}} {{usage}}\n\n{}\nOptions:\n{{options}}\n\n{}",
        crate::help::command_tree(crate::help::Surface::Cli),
        crate::help::reference()
    )
}

#[derive(Parser, Debug)]
#[command(name = "riveter")]
#[command(version)]
#[command(help_template = help_template())]
// Replaced by an explicit `Help` variant so it can carry the `h` alias the
// REPL also accepts.
#[command(disable_help_subcommand = true)]
pub struct Cli {
    /// Environment to act on, overriding `env set` and `RIVETER_ENV`.
    ///
    /// The environment recorded by `env set` is shared state in the working
    /// directory; naming it here pins one invocation to one environment.
    #[arg(long, short = 'e', global = true, value_name = "ENV")]
    pub env: Option<String>,

    #[command(subcommand)]
    pub cmd: Option<Cmd>,
}

const SCOPE_HELP: &str = "Which resources to include: `mutable` skips those marked \
                          immutable, `immutable` selects only those, `all` takes \
                          everything.";

#[derive(Subcommand, Debug)]
pub enum Cmd {
    /// Manage environments
    Env {
        #[command(subcommand)]
        cmd: EnvCmd,
    },
    /// List the resources the current environment declares
    #[command(visible_alias = "ls")]
    List {
        #[arg(long, value_enum, default_value_t = ApplyScope::All, help = SCOPE_HELP)]
        scope: ApplyScope,
        #[arg(value_name = "TARGET", help = TARGET_HELP)]
        targets: Vec<String>,
    },
    /// Render manifests into manifests/
    ///
    /// Defaults to the same scope as `apply`, so a render previews exactly what
    /// an apply would send. Writes `manifests/<env>-manifests.<scope>.yaml`, or
    /// `-manifests.selection.yaml` when targets are given, so the full manifest
    /// is never overwritten.
    #[command(visible_alias = "r")]
    Render {
        #[arg(long, value_enum, default_value_t = ApplyScope::Mutable, help = SCOPE_HELP)]
        scope: ApplyScope,
        #[arg(value_name = "TARGET", help = TARGET_HELP)]
        targets: Vec<String>,
    },
    /// Render the selected resources and apply them with kubectl
    #[command(visible_alias = "a")]
    Apply {
        /// Pass --dry-run=client to kubectl; nothing reaches the cluster
        #[arg(long)]
        dry_run: bool,
        /// Return as soon as kubectl accepts the manifests, without waiting for
        /// the rollout to become ready
        #[arg(long)]
        no_wait: bool,
        /// Seconds to wait for each rollout before giving up
        #[arg(long, value_name = "SECONDS", default_value_t = 300)]
        timeout: u64,
        #[arg(long, value_enum, default_value_t = ApplyScope::Mutable, help = SCOPE_HELP)]
        scope: ApplyScope,
        #[arg(value_name = "TARGET", help = TARGET_HELP)]
        targets: Vec<String>,
    },
    /// Show what applying would change, via `kubectl diff`
    #[command(visible_alias = "df")]
    Diff {
        #[arg(long, value_enum, default_value_t = ApplyScope::Mutable, help = SCOPE_HELP)]
        scope: ApplyScope,
        #[arg(value_name = "TARGET", help = TARGET_HELP)]
        targets: Vec<String>,
    },
    /// Delete cluster resources riveter manages that the overlay no longer declares
    ///
    /// Finds live resources labelled as belonging to this environment, compares
    /// them against what the overlay renders, and removes the difference.
    Prune {
        /// List what would be pruned without deleting anything
        #[arg(long)]
        dry_run: bool,
    },
    /// Render the selected resources and delete them with kubectl
    #[command(visible_aliases = ["d", "del"])]
    Delete {
        #[arg(long, value_enum, default_value_t = ApplyScope::Mutable, help = SCOPE_HELP)]
        scope: ApplyScope,
        #[arg(value_name = "TARGET", help = TARGET_HELP)]
        targets: Vec<String>,
    },
    /// Check overlay deployment image tags for newer registry tags
    Images {
        /// Rewrite deployment templates in place to the newest compatible tag found
        #[arg(long)]
        update: bool,
        /// Overlay directory to scan
        #[arg(long, value_name = "DIR")]
        overlays_dir: Option<std::path::PathBuf>,
        /// Registry credentials, repeatable; prefer `RIVETER_REGISTRY_AUTH` or
        /// Docker config to avoid shell history
        #[arg(long = "registry-auth", value_name = "REGISTRY=USER:PASS")]
        registry_auth: Vec<String>,
    },
    /// Start the interactive REPL (also the default with no arguments)
    Repl,
    /// Show help, or detail for one command
    #[command(visible_alias = "h")]
    Help {
        /// Command to describe; omit for the full command tree
        command: Option<String>,
    },
}

#[derive(ValueEnum, Clone, Copy, Debug)]
pub enum ApplyScope {
    /// Skip resources marked `immutable: true` or `lifecycle: immutable`
    Mutable,
    /// Only resources marked immutable
    Immutable,
    /// Every resource
    All,
}

#[derive(Subcommand, Debug)]
pub enum EnvCmd {
    /// List available environments
    List,
    /// Set the current environment
    Set {
        /// Name of a directory under overlays/ holding an overlay.yaml
        env: String,
    },
    /// Show the current environment
    Show,
}
