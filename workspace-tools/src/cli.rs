use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "wst")]
#[command(about = "Workspace tools for building, linting, and publishing", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Build the workspace or specific packages
    Build {
        /// Build all packages
        #[arg(long)]
        all: bool,
        /// Build with all features
        #[arg(long)]
        all_features: bool,
        /// Build in release mode
        #[arg(long)]
        release: bool,
        /// Specific package to build
        #[arg(short, long)]
        package: Option<String>,
    },
    /// Lint the workspace with clippy
    Lint {
        /// Apply clippy to all targets
        #[arg(long, default_value = "true")]
        all_targets: bool,
        /// Apply clippy with all features
        #[arg(long, default_value = "true")]
        all_features: bool,
        /// Treat warnings as errors
        #[arg(long, default_value = "true")]
        deny_warnings: bool,
    },
    /// Format code with rustfmt
    Format {
        /// Check formatting without applying changes
        #[arg(long)]
        check: bool,
    },
    /// List all workspace packages
    List {
        /// Output format (json, names)
        #[arg(long, default_value = "names")]
        format: String,
    },
    /// Upgrade workspace dependencies
    Upgrade {
        /// Allow incompatible upgrades
        #[arg(long)]
        incompatible: bool,
    },
    /// Audit dependencies for security vulnerabilities
    Audit,
    /// Find unused dependencies with cargo-machete
    Machete,
    /// Test the workspace
    Test {
        /// Run tests for all packages
        #[arg(long)]
        all: bool,
        /// Specific package to test
        #[arg(short, long)]
        package: Option<String>,
    },
    /// Build and publish Docker images
    Docker {
        #[command(subcommand)]
        command: DockerCommands,
    },
}

#[derive(Subcommand)]
pub enum DockerCommands {
    /// Build Docker image for a package
    Build {
        /// Package name to build
        #[arg(short, long)]
        package: String,
    },
    /// Tag Docker image
    Tag {
        /// Package name
        #[arg(short, long)]
        package: String,
        /// Docker registry repository path
        #[arg(long, default_value = "ossiriand.arda:30021/ossiriand-1/ossiriand")]
        registry: String,
    },
    /// Push Docker image to registry
    Push {
        /// Package name
        #[arg(short, long)]
        package: String,
        /// Docker registry repository path
        #[arg(long, default_value = "ossiriand.arda:30021/ossiriand-1/ossiriand")]
        registry: String,
    },
    /// Build, tag, and push Docker image
    Release {
        /// Package name
        #[arg(short, long)]
        package: String,
        /// Docker registry repository path
        #[arg(long, default_value = "ossiriand.arda:30021/ossiriand-1/ossiriand")]
        registry: String,
    },
    /// Build all Docker images
    BuildAll,
    /// Build, tag, and push all Docker images
    ReleaseAll {
        /// Docker registry repository path
        #[arg(long, default_value = "ossiriand.arda:30021/ossiriand-1/ossiriand")]
        registry: String,
    },
}
