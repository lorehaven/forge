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
    /// Build the current environment's overlay into a `.rivet` package
    Pack {
        /// Build metadata appended to the manifest's version, e.g. `b123` makes `0.4.0` into `0.4.0+b123`; `{timestamp}` and `{sha}` expand to the UTC time and the commit
        #[arg(long, value_name = "SUFFIX")]
        version_suffix: Option<String>,
        /// Do not pin image tags to digests; packing then needs no registry access
        #[arg(long)]
        no_pin: bool,
        /// Directory to write the package to
        #[arg(long, value_name = "DIR", default_value = "packages")]
        out: std::path::PathBuf,
        /// Registry credentials for digest lookups, repeatable
        #[arg(long = "registry-auth", value_name = "REGISTRY=USER:PASS")]
        registry_auth: Vec<String>,
    },
    /// Publish a package to Warehouse, packing the current environment first if no file is given
    Publish {
        /// A `.rivet` file to publish; omit to pack the current environment
        #[arg(value_name = "FILE")]
        file: Option<std::path::PathBuf>,
        /// Build metadata appended to the version when packing; takes `{timestamp}` and `{sha}`
        #[arg(long, value_name = "SUFFIX")]
        version_suffix: Option<String>,
        /// Publish nothing when the newest published version has the same content (ignoring its build metadata)
        #[arg(long)]
        skip_unchanged: bool,
        /// Do not pin image tags to digests when packing
        #[arg(long)]
        no_pin: bool,
        /// Directory to write the package to when packing
        #[arg(long, value_name = "DIR", default_value = "packages")]
        out: std::path::PathBuf,
        /// Registry credentials for digest lookups, repeatable
        #[arg(long = "registry-auth", value_name = "REGISTRY=USER:PASS")]
        registry_auth: Vec<String>,
    },
    /// Download a package from Warehouse and verify it
    Pull {
        /// `name`, `name@version` or `name@latest`
        #[arg(value_name = "PACKAGE")]
        package: String,
        /// Directory to save the package in
        #[arg(long, value_name = "DIR", default_value = "packages")]
        out: std::path::PathBuf,
    },
    /// Install a package: fetch it, render it and apply it with kubectl
    ///
    /// The package may be `name`, `name@version`, or the path of a `.rivet`
    /// file. Variables come from the package's own `values.toml`, then
    /// `--env-file`, then `--set`; the working directory's `.env` is not read.
    #[command(visible_alias = "i")]
    Install {
        /// `name[@version]` from Warehouse, or a `.rivet` file
        #[arg(value_name = "PACKAGE")]
        package: String,
        /// A dotenv-format file of variables, e.g. the overlay's own `.env`
        #[arg(long, value_name = "FILE")]
        env_file: Option<std::path::PathBuf>,
        /// One variable as KEY=value, repeatable; wins over everything else
        #[arg(long = "set", value_name = "KEY=VALUE")]
        sets: Vec<String>,
        /// Set a deployment's or statefulset's replicas whatever the package says, as kind/name=N, repeatable; `deployment/sage=0` installs it stopped
        #[arg(long = "replicas", value_name = "KIND/NAME=N")]
        replicas: Vec<String>,
        /// Leave these resources out (kind/name, repeatable): how a service is upgraded last, on its own, with a way back
        #[arg(long = "except", value_name = "KIND/NAME")]
        except: Vec<String>,
        /// Also print everything the package declares as one JSON line (`riveter-inventory: [...]`), whatever was selected
        #[arg(long)]
        inventory: bool,
        /// Pass --dry-run=client to kubectl; nothing reaches the cluster
        #[arg(long)]
        dry_run: bool,
        /// Return as soon as kubectl accepts the manifests, without waiting for the rollout
        #[arg(long)]
        no_wait: bool,
        /// Seconds to wait for each rollout before giving up
        #[arg(long, value_name = "SECONDS", default_value_t = 300)]
        timeout: u64,
        #[arg(long, value_enum, default_value_t = ApplyScope::All, help = SCOPE_HELP)]
        scope: ApplyScope,
        #[arg(value_name = "TARGET", help = TARGET_HELP)]
        targets: Vec<String>,
    },
    /// Check the environment's custom resources against their CRD schemas, offline
    ///
    /// Renders the environment in memory and checks each resource whose `apiVersion`/`kind` has a schema
    /// held - cert-manager, Traefik and Gateway API ship in the binary, and `schemas fetch` adds more from
    /// a cluster - the way the API server will: a typo in a field name is an error, with a suggestion.
    /// Kinds with no schema are counted, not failed. Nothing is sent to a cluster.
    Validate {
        #[arg(long, value_enum, default_value_t = ApplyScope::All, help = SCOPE_HELP)]
        scope: ApplyScope,
        /// Check the documents in these files instead of the environment (`-` is standard input)
        #[arg(long = "file", short = 'f', value_name = "FILE")]
        files: Vec<std::path::PathBuf>,
        #[arg(value_name = "TARGET", help = TARGET_HELP)]
        targets: Vec<String>,
    },
    /// Put overlays' `.env` files in the cluster as Secrets, for installs to read
    Secrets {
        #[command(subcommand)]
        cmd: SecretsCmd,
    },
    /// Manage the CRD schemas `validate` checks against
    Schemas {
        #[command(subcommand)]
        cmd: SchemasCmd,
    },
    /// Browse the packages in Warehouse
    Remote {
        #[command(subcommand)]
        cmd: RemoteCmd,
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
pub enum SecretsCmd {
    /// Make an age key pair; the private key is what opens every encrypted file
    ///
    /// Writes the private key to `--out` (mode 0600, refusing to overwrite) or prints it, and prints the
    /// public recipient to give to `set` and `import`. Keep the private key out of git: it is the one thing
    /// to back up, and the one thing a rebuilt machine needs.
    Keygen {
        /// File to write the private key to
        #[arg(long, value_name = "FILE")]
        out: Option<std::path::PathBuf>,
    },
    /// Encrypt one value into a secrets file
    ///
    /// The value comes from `--value`, or from standard input (so it stays out of shell history). Every other
    /// value in the file is left exactly as it was.
    Set {
        /// The secrets file (`secrets.yaml` in the overlay)
        #[arg(value_name = "FILE")]
        file: std::path::PathBuf,
        /// Variable name
        #[arg(value_name = "NAME")]
        name: String,
        /// The value; omit to read it from standard input
        #[arg(long, value_name = "VALUE")]
        value: Option<String>,
        /// Public key(s) to encrypt to, for a new file; an existing file keeps its own
        #[arg(long = "recipient", short = 'r', value_name = "AGE1...")]
        recipients: Vec<String>,
    },
    /// Move values from a dotenv file into a secrets file, encrypted
    Import {
        /// The secrets file
        #[arg(value_name = "FILE")]
        file: std::path::PathBuf,
        /// The dotenv file to read
        #[arg(long = "from", value_name = "DOTENV")]
        from: std::path::PathBuf,
        /// Only these names (default: all of them)
        #[arg(value_name = "NAME")]
        names: Vec<String>,
        /// Public key(s) to encrypt to, for a new file
        #[arg(long = "recipient", short = 'r', value_name = "AGE1...")]
        recipients: Vec<String>,
    },
    /// List the names in a secrets file (never the values)
    #[command(visible_alias = "ls")]
    List {
        #[arg(value_name = "FILE")]
        file: std::path::PathBuf,
    },
    /// Print one value, decrypted, to standard output
    Show {
        #[arg(value_name = "FILE")]
        file: std::path::PathBuf,
        #[arg(value_name = "NAME")]
        name: String,
    },
    /// Remove a name from a secrets file
    Remove {
        #[arg(value_name = "FILE")]
        file: std::path::PathBuf,
        #[arg(value_name = "NAME")]
        name: String,
    },
    /// Encrypt a file to different recipients, after a key is lost or someone should lose access
    Rekey {
        #[arg(value_name = "FILE")]
        file: std::path::PathBuf,
        /// The new recipients (replaces the old ones)
        #[arg(
            long = "recipient",
            short = 'r',
            value_name = "AGE1...",
            required = true
        )]
        recipients: Vec<String>,
    },
    /// Write each named overlay's `.env` to a Secret `gantry-values-<overlay>`
    ///
    /// One Secret per overlay, holding the file as it is under the key `env`, written with your own kubectl
    /// access. Values go to kubectl over standard input and are never printed: only Secret and variable
    /// names are. `--all` takes every overlay that has a `.env`.
    Sync {
        /// Overlays to sync, by name
        #[arg(value_name = "OVERLAY")]
        overlays: Vec<String>,
        /// Every overlay that has a `.env`
        #[arg(long)]
        all: bool,
        /// Namespace the Secrets go in
        #[arg(long, short = 'n', value_name = "NAMESPACE", default_value = "forge")]
        namespace: String,
        /// kubectl context to write to, instead of the current one
        #[arg(long, value_name = "CONTEXT")]
        context: Option<String>,
        /// Report what would be synced without touching the cluster
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum SchemasCmd {
    /// List the schemas held, and where each came from
    #[command(visible_alias = "ls")]
    List,
    /// Read CRD schemas from a cluster into the cache, where they take precedence over the bundled ones
    ///
    /// By default the CRDs riveter has templates for. `--crd` names others; `--all` takes every CRD in
    /// the cluster, so `raw` resources of any kind can be checked.
    Fetch {
        /// A CRD by name (`certificates.cert-manager.io`); repeatable
        #[arg(long, value_name = "NAME")]
        crd: Vec<String>,
        /// Every CRD in the cluster
        #[arg(long)]
        all: bool,
        /// kubectl context to read from, instead of the current one
        #[arg(long, value_name = "CONTEXT")]
        context: Option<String>,
        /// Write here instead of the cache directory
        #[arg(long, value_name = "DIR")]
        output: Option<std::path::PathBuf>,
    },
}

#[derive(Subcommand, Debug)]
pub enum RemoteCmd {
    /// List the newest version of every package
    #[command(visible_alias = "ls")]
    List,
    /// List every version of one package
    Versions {
        /// Package name
        name: String,
    },
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
