use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(name = "warehouse", version, about = "Warehouse CLI")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Manage and query a Docker container registry
    Docker {
        #[command(subcommand)]
        command: DockerCommands,
    },
    /// Manage and query a Cargo crates registry
    Crates {
        #[command(subcommand)]
        command: CratesCommands,
    },
}

// ---------------------------------------------------------------------------
// Docker
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Crates
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
pub enum CratesCommands {
    /// Manage configured crates registries
    Registry {
        #[command(subcommand)]
        command: CratesRegistryCommands,
    },
    /// Save an API token for a registry
    Login(CratesLoginArgs),
    /// Search for crates by name
    Search(CratesSearchArgs),
    /// List published versions of a crate (reads the sparse index)
    Versions(CratesVersionsArgs),
    /// Yank a published crate version
    Yank(CratesYankArgs),
    /// Un-yank a previously yanked crate version
    Unyank(CratesUnyankArgs),
}

// ---------------------------------------------------------------------------
// Shared registry management (used by both Docker and Crates subcommands
// via their respective RegistryCommands / CratesRegistryCommands)
// ---------------------------------------------------------------------------

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

#[derive(Subcommand)]
pub enum CratesRegistryCommands {
    /// Add or update a crates registry config
    Add(CratesRegistryAddArgs),
    /// List configured crates registries
    List,
    /// Set active crates registry
    Use(CratesRegistryUseArgs),
    /// Remove a crates registry
    Remove(CratesRegistryRemoveArgs),
}

// ---------------------------------------------------------------------------
// Docker args
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Crates args
// ---------------------------------------------------------------------------

#[derive(Args)]
pub struct CratesRegistryAddArgs {
    /// Registry name used in local configuration
    pub name: String,
    /// Registry base URL, e.g. https://registry.example.com
    #[arg(long)]
    pub url: String,
    /// Skip TLS certificate validation
    #[arg(long)]
    pub insecure_tls: bool,
    /// Set as current active crates registry
    #[arg(long)]
    pub r#use: bool,
    /// Write to ~/.config/warehouse instead of .warehouse
    #[arg(long)]
    pub global: bool,
}

#[derive(Args)]
pub struct CratesRegistryUseArgs {
    /// Registry name
    pub name: String,
    /// Update ~/.config/warehouse instead of .warehouse
    #[arg(long)]
    pub global: bool,
}

#[derive(Args)]
pub struct CratesRegistryRemoveArgs {
    /// Registry name
    pub name: String,
    /// Remove from ~/.config/warehouse instead of .warehouse
    #[arg(long)]
    pub global: bool,
}

#[derive(Args)]
pub struct CratesLoginArgs {
    /// Registry name; defaults to active crates registry from config
    #[arg(long)]
    pub registry: Option<String>,
    /// API token (equivalent of `cargo login` token)
    #[arg(long)]
    pub token: String,
    /// Save token to ~/.config/warehouse instead of .warehouse
    #[arg(long)]
    pub global: bool,
}

#[derive(Args)]
pub struct CratesSearchArgs {
    /// Search query string
    pub query: String,
    /// Registry name; defaults to active crates registry from config
    #[arg(long)]
    pub registry: Option<String>,
    /// Maximum results to return
    #[arg(long, default_value_t = 10)]
    pub limit: usize,
}

#[derive(Args)]
pub struct CratesVersionsArgs {
    /// Crate name
    pub crate_name: String,
    /// Registry name; defaults to active crates registry from config
    #[arg(long)]
    pub registry: Option<String>,
    /// Show all versions including yanked ones
    #[arg(long)]
    pub all: bool,
}

#[derive(Args)]
pub struct CratesYankArgs {
    /// Crate name
    pub crate_name: String,
    /// Version to yank
    pub version: String,
    /// Registry name; defaults to active crates registry from config
    #[arg(long)]
    pub registry: Option<String>,
}

#[derive(Args)]
pub struct CratesUnyankArgs {
    /// Crate name
    pub crate_name: String,
    /// Version to un-yank
    pub version: String,
    /// Registry name; defaults to active crates registry from config
    #[arg(long)]
    pub registry: Option<String>,
}
