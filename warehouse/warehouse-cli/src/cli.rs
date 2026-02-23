use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(name = "warehouse", version, about = "Warehouse CLI")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    Docker {
        #[command(subcommand)]
        command: DockerCommands,
    },
}

#[derive(Subcommand)]
pub enum DockerCommands {
    /// Manage configured registries
    Registry {
        #[command(subcommand)]
        command: RegistryCommands,
    },
    /// Save credentials for a registry
    Login(LoginArgs),
    /// List repositories in the docker registry catalog
    Catalog(CatalogArgs),
    /// List tags for a repository
    Tags(TagsArgs),
}

#[derive(Subcommand)]
pub enum RegistryCommands {
    /// Add or update a registry config
    Add(RegistryAddArgs),
    /// List configured registries
    List,
    /// Set active registry
    Use(RegistryUseArgs),
    /// Remove a registry
    Remove(RegistryRemoveArgs),
}

#[derive(Args)]
pub struct RegistryAddArgs {
    /// Registry name used in local configuration
    pub name: String,
    /// Registry base URL, e.g. http://registry.local:8698
    #[arg(long)]
    pub url: String,
    /// Docker API path segment, defaults to /v2
    #[arg(long, default_value = "/v2")]
    pub path: String,
    /// Docker auth service value; if omitted, derived from challenge or host
    #[arg(long)]
    pub service: Option<String>,
    /// Skip TLS certificate validation for this registry
    #[arg(long)]
    pub insecure_tls: bool,
    /// Set as current active registry
    #[arg(long)]
    pub r#use: bool,
    /// Write to ~/.config/warehouse instead of .warehouse
    #[arg(long)]
    pub global: bool,
}

#[derive(Args)]
pub struct RegistryUseArgs {
    /// Registry name
    pub name: String,
    /// Update ~/.config/warehouse instead of .warehouse
    #[arg(long)]
    pub global: bool,
}

#[derive(Args)]
pub struct RegistryRemoveArgs {
    /// Registry name
    pub name: String,
    /// Remove from ~/.config/warehouse instead of .warehouse
    #[arg(long)]
    pub global: bool,
}

#[derive(Args)]
pub struct LoginArgs {
    /// Registry name; defaults to active registry from config
    #[arg(long)]
    pub registry: Option<String>,
    /// Username for basic auth to token realm
    #[arg(long)]
    pub username: String,
    /// Password for basic auth to token realm
    #[arg(long)]
    pub password: String,
    /// Save credentials to ~/.config/warehouse instead of .warehouse
    #[arg(long)]
    pub global: bool,
}

#[derive(Args)]
pub struct CatalogArgs {
    /// Registry name; defaults to active registry from config
    #[arg(long)]
    pub registry: Option<String>,
    /// Maximum repositories to fetch in one page
    #[arg(long, default_value_t = 100)]
    pub n: usize,
}

#[derive(Args)]
pub struct TagsArgs {
    /// Repository name
    pub repository: String,
    /// Registry name; defaults to active registry from config
    #[arg(long)]
    pub registry: Option<String>,
    /// Maximum tags to fetch in one page
    #[arg(long, default_value_t = 100)]
    pub n: usize,
}

impl Commands {
    pub fn into_docker(self) -> DockerCommands {
        match self {
            Commands::Docker { command } => command,
        }
    }
}
