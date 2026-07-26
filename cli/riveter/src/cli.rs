use clap::{Parser, Subcommand, ValueEnum};

const TARGET_HELP: &str = "Resources to act on as kind[/name], e.g. `deployment/api`, \
                           `statefulset` or `*/api`. Both halves accept `*` and `?` \
                           wildcards. Omit to act on every resource in scope.";

#[derive(Parser, Debug)]
#[command(name = "riveter")]
#[command(version)]
pub struct Cli {
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
    /// Writes `manifests/<env>-manifests.yaml`, or `-manifests.selection.yaml`
    /// when targets are given, so the full manifest is never overwritten.
    #[command(visible_alias = "r")]
    Render {
        #[arg(long, value_enum, default_value_t = ApplyScope::All, help = SCOPE_HELP)]
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
        #[arg(long, value_enum, default_value_t = ApplyScope::Mutable, help = SCOPE_HELP)]
        scope: ApplyScope,
        #[arg(value_name = "TARGET", help = TARGET_HELP)]
        targets: Vec<String>,
    },
    /// Render the selected resources and delete them with kubectl
    #[command(visible_aliases = ["d", "del"])]
    Delete {
        #[arg(long, value_enum, default_value_t = ApplyScope::Mutable, help = SCOPE_HELP)]
        scope: ApplyScope,
        #[arg(value_name = "TARGET", help = TARGET_HELP)]
        targets: Vec<String>,
    },
    /// Start the interactive REPL (also the default with no arguments)
    Repl,
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
