use quench_args::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "anvil")]
#[command(version)]
#[command(about = "Anvil - Workspace tools for building, linting, and publishing", long_about = None)]
pub struct Cli {
    /// Suppress the underlying command's live output; only anvil's own
    /// status lines are shown (a short tail is still printed on failure)
    #[arg(long, global = true, conflicts_with = "tail")]
    pub silent: bool,
    /// Instead of streaming the underlying command's output live, print only
    /// the last N lines once it finishes (default 80 when no value is given).
    /// A value, if given, must be attached with `=` (e.g. `--tail=200`) so it
    /// isn't mistaken for the subcommand that follows.
    #[arg(
        long,
        global = true,
        value_name = "N",
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "80"
    )]
    pub tail: Option<usize>,
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
        /// Emit clippy's own `--message-format=json` diagnostics instead of streaming its human output
        #[arg(long)]
        json: bool,
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
    Audit {
        /// Emit cargo-audit's own `--format json` report instead of streaming its human output
        #[arg(long)]
        json: bool,
    },
    /// Find unused dependencies with cargo-machete
    Machete {
        /// Emit a JSON array of findings instead of streaming cargo-machete's human output
        /// (cargo-machete has no `--json` of its own, so this parses its plain-text report)
        #[arg(long)]
        json: bool,
    },
    /// Check licenses, banned/duplicate crates, and registry sources with cargo-deny
    Deny {
        /// Emit cargo-deny's own `--format json` diagnostics instead of streaming its human output
        #[arg(long)]
        json: bool,
    },
    /// Check a library crate's public API for unflagged breaking changes with cargo-semver-checks
    ///
    /// Diffs against a git revision rather than a registry version, since the
    /// crates this workspace publishes live on a private registry that
    /// cargo-semver-checks can't query directly. A package on its first
    /// release - no earlier commit to diff against - instead compares
    /// against a synthetic empty API, so nothing reads as a breaking change.
    SemverCheck {
        /// Package to check (must have a library target)
        #[arg(short, long)]
        package: String,
        /// Git revision to diff against (defaults to the last commit that
        /// changed the package's Cargo.toml, or an empty API on its first release)
        #[arg(long)]
        baseline_rev: Option<String>,
    },
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
        /// Emit per-test JSON results instead of streaming human output.
        /// Runs under `+nightly` with `-Z unstable-options` - the built-in
        /// libtest harness has no stable structured-output format.
        #[arg(long)]
        json: bool,
    },
    /// Test the workspace with cargo-nextest (faster, parallel-by-default runner)
    Nextest {
        /// Run tests for all packages
        #[arg(long)]
        all: bool,
        /// Specific package to test
        #[arg(short, long)]
        package: Option<String>,
        /// Optional test name filter
        test_name: Option<String>,
        /// Run only ignored tests
        #[arg(long)]
        ignored: bool,
        /// Emit per-test JSON results (nextest's experimental `libtest-json-plus`
        /// format) instead of streaming human output
        #[arg(long)]
        json: bool,
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
    /// Release package(s): bump patch version, commit, then docker release or cargo publish+install
    Release {
        /// Release all packages configured in [publish].packages
        #[arg(long, conflicts_with = "package")]
        all: bool,
        /// Specific package to release
        #[arg(short, long)]
        package: Option<String>,
        /// Preview what would be released without changing files or publishing
        #[arg(long)]
        dry_run: bool,
        /// Do not ask for confirmation (needed when nothing can answer, as in a pipeline)
        #[arg(short = 'y', long)]
        yes: bool,
        /// Leave out the Docker packages (their images are built and pushed elsewhere): only crates are released.
        /// For a job that has no Docker daemon
        #[arg(long)]
        no_docker: bool,
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
    /// Inspect or validate the configuration (`.anvil.toml` and `.anvil/*.toml`)
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
    },
    /// Build a program's apps and desktop packages and send them to the warehouse
    Artifact {
        #[command(subcommand)]
        command: ArtifactCommands,
    },
    /// Check, build and release documentation books (Jekyll sites of Markdown files)
    Docs {
        #[command(subcommand)]
        command: DocsCommands,
    },
    /// Print a registry bearer token, for Cargo's `credential-provider = [\"cargo:token-from-stdout\", \"anvil\", \"registry-token\"]`
    RegistryToken,
    /// Store the Android release keystore's password in the git-ignored secret file (after `anvil login`), so
    /// `anvil artifact release` signs without `QUENCH_KEYSTORE_PASSWORD` in the environment
    KeystorePassword {
        /// Ask for the key's own password too (when it is not the keystore's)
        #[arg(long)]
        key: bool,
    },
    /// Mint an application token and store it in the git-ignored `.anvil/secret.toml` (`.anvil.secret.toml` when there is no `.anvil/`)
    Login {
        /// Gatehouse including its base path (defaults to the one already stored)
        #[arg(long)]
        gatehouse_url: Option<String>,
        /// Gatehouse username (asked for when omitted)
        #[arg(long)]
        username: Option<String>,
        /// Name shown next to the token in Gatehouse
        #[arg(long)]
        name: Option<String>,
        /// Limit the token to a `service:action` grant, e.g. `warehouse:write` (repeatable)
        #[arg(long = "scope")]
        scope: Vec<String>,
        /// Days until the token expires (default and maximum: 365)
        #[arg(long)]
        days: Option<i64>,
    },
}

#[derive(Subcommand, Debug)]
pub enum ArtifactCommands {
    /// Build every target of a program (or one with --target) and publish it to the warehouse
    Release {
        /// The program, as named under [artifacts.programs] in the configuration (or --all)
        #[arg(conflicts_with = "all")]
        name: Option<String>,
        /// Release every program of [artifacts.programs], each with all its targets, in name order
        #[arg(long)]
        all: bool,
        /// Do not ask for confirmation (needed with --all when nothing can answer, as in a pipeline)
        #[arg(short = 'y', long)]
        yes: bool,
        /// A program whose version is already in the warehouse for every build gets its patch version raised
        /// (in `version_file`, committed) and is released as the new version; one that is not there yet is
        /// released as it is. On by default with --all
        #[arg(long, conflicts_with = "no_bump")]
        bump: bool,
        /// With --all: do not raise versions (a version the warehouse already has is an error)
        #[arg(long)]
        no_bump: bool,
        /// Only this target (`android`, `linux`, ...)
        #[arg(long)]
        target: Option<String>,
        /// Build and check everything, but send nothing
        #[arg(long)]
        dry_run: bool,
        /// Release with uncommitted changes in the tree
        #[arg(long)]
        allow_dirty: bool,
        /// A version the warehouse already has is skipped instead of an error
        #[arg(long)]
        skip_existing: bool,
    },
    /// List what can be released
    List,
}

#[derive(Subcommand, Debug)]
pub enum DocsCommands {
    /// Check the books without building them: front matter, navigation, links and `OpenAPI` documents against the source
    Check {
        /// The book, as named under [docs.books] (every book when left out)
        book: Option<String>,
    },
    /// Check a book, build it with Jekyll and pack the site as <out>/<book>/<book>-<version>.tar.gz with a manifest.json
    Build {
        /// The book, as named under [docs.books] (every book when left out)
        book: Option<String>,
        /// Use the theme the book's `_config.yml` names (a remote theme: needs the `jekyll-remote-theme` gem and the network) instead of the installed `just-the-docs` gem
        #[arg(long)]
        remote_theme: bool,
        /// Skip the checks
        #[arg(long)]
        no_check: bool,
        /// Where the builds go (default: `out` under [docs], else `target/docs`)
        #[arg(long)]
        out: Option<String>,
        /// The `baseurl` the site is built for (default: the book's)
        #[arg(long)]
        baseurl: Option<String>,
    },
    /// Build the books and send them to the warehouse files storage as <book>/<version>/ and <book>/latest.json
    Release {
        /// The book, as named under [docs.books] (every book when left out)
        book: Option<String>,
        /// Build and check everything, but send nothing
        #[arg(long)]
        dry_run: bool,
        /// Use the theme the book's `_config.yml` names (a remote theme: needs the `jekyll-remote-theme` gem and the network) instead of the installed `just-the-docs` gem
        #[arg(long)]
        remote_theme: bool,
        /// Skip the checks
        #[arg(long)]
        no_check: bool,
        /// The `baseurl` the site is built for (default: the book's)
        #[arg(long)]
        baseurl: Option<String>,
    },
    /// List the books
    List,
}

#[derive(Subcommand, Debug)]
pub enum ConfigCommands {
    /// Validate the configuration (every file, merged) against anvil's schema, reporting every error at once
    Check,
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
