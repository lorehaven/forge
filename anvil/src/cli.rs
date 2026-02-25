use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "anvil")]
#[command(about = "Anvil - Workspace tools for building, linting, and publishing", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
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
    /// Clean the workspace or packages
    Clean,
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
        /// Optional test name filter (same as cargo test TESTNAME)
        test_name: Option<String>,
        /// Run ignored tests (same as cargo test -- --ignored)
        #[arg(long)]
        ignored: bool,
        /// List available tests (same as cargo test -- --list)
        #[arg(long)]
        list: bool,
    },
    /// Install a package binary with cargo install --path
    Install {
        /// Install all workspace packages
        #[arg(long, conflicts_with = "package")]
        all: bool,
        /// Specific package to install (required for multi-package workspace roots)
        #[arg(short, long)]
        package: Option<String>,
    },
    /// Build and run a package/binary
    Run {
        /// Specific package to run
        #[arg(short, long)]
        package: Option<String>,
        /// Build and serve mode: watch for file changes and rebuild/restart
        /// Hotkeys in serve mode: `r` rebuild now, `R` toggle auto-rebuild, `q/Q/e/E` quit
        #[arg(long)]
        serve: bool,
        /// Polling interval in milliseconds for serve mode
        #[arg(long, default_value_t = 1000)]
        watch_interval_ms: u64,
    },
    /// Build and publish Docker images
    Docker {
        #[command(subcommand)]
        command: DockerCommands,
    },
}

#[derive(Subcommand, Debug)]
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
    },
    /// Push Docker image to registry
    Push {
        /// Package name
        #[arg(short, long)]
        package: String,
    },
    /// Build, tag, and push Docker image
    Release {
        /// Package name
        #[arg(short, long)]
        package: String,
    },
    /// Build all Docker images
    BuildAll,
    /// Build, tag, and push all Docker images
    ReleaseAll,
}
