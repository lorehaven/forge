//! REPL help text.
//!
//! The overview and the per-command detail are generated from one [`COMMANDS`]
//! table, so `help` and `help <command>` cannot drift apart.

use std::fmt::Write as _;

/// Where a command exists. `riveter repl` is CLI-only, `exit` is REPL-only,
/// and everything else works on both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    Cli,
    Repl,
    Both,
}

impl Surface {
    const fn shows(self, on: Self) -> bool {
        matches!(self, Self::Both)
            || matches!(
                (self, on),
                (Self::Cli, Self::Cli) | (Self::Repl, Self::Repl)
            )
    }
}

/// A command as the help menu describes it.
#[derive(Debug)]
pub struct CommandHelp {
    pub name: &'static str,
    pub surface: Surface,
    pub aliases: &'static [&'static str],
    /// Argument spec shown after the name, e.g. `[--scope <scope>] [target...]`.
    pub usage: &'static str,
    pub summary: &'static str,
    /// Longer prose for `help <command>`.
    pub detail: &'static str,
    pub subcommands: &'static [(&'static str, &'static str)],
    pub options: &'static [(&'static str, &'static str)],
    pub examples: &'static [(&'static str, &'static str)],
    /// Whether the command accepts `kind[/name]` targets.
    pub targets: bool,
}

const SCOPE_ALL: &str = "mutable | immutable | all (default: all)";
const SCOPE_MUTABLE: &str = "mutable | immutable | all (default: mutable)";

pub const TARGETS: &str = "\
A target is kind[/name] — `deployment/api`, `statefulset`, `*/api`.
Both halves accept `*` and `?` wildcards and are matched case-insensitively;
kind aliases (sts, ds, hpa, pdb, crd, netpol, sa) resolve to their canonical
kind. Quote patterns so the shell does not expand them.

Without targets a command acts on every resource in scope. A target that
matches nothing is an error listing the available resources.";

const SCOPES: &str = "\
Scopes:
  mutable     skip resources marked `immutable: true` or `lifecycle: immutable`
  immutable   only those resources
  all         everything";

pub const COMMANDS: &[CommandHelp] = &[
    CommandHelp {
        name: "env",
        surface: Surface::Both,
        aliases: &[],
        usage: "<list|set|show>",
        summary: "Manage environments",
        detail: "An environment is a directory under `overlays/` containing an\n\
                 `overlay.yaml`. The selected one is stored in `.riveter.toml`.",
        subcommands: &[
            ("list", "List available environments"),
            ("set <env>", "Set the current environment"),
            ("show", "Show the current environment"),
        ],
        options: &[],
        examples: &[
            ("env list", "show every overlay"),
            ("env set prod", "switch to overlays/prod"),
        ],
        targets: false,
    },
    CommandHelp {
        name: "list",
        surface: Surface::Both,
        aliases: &["ls"],
        usage: "[options] [target...]",
        summary: "List the environment's resources",
        detail: "Reads the overlay and prints each resource as kind, name and\n\
                 lifecycle, without rendering any templates.",
        subcommands: &[],
        options: &[("--scope <scope>", SCOPE_ALL)],
        examples: &[
            ("list", "every resource"),
            ("list --scope immutable", "what a default apply would skip"),
            ("list statefulset", "only statefulsets"),
        ],
        targets: true,
    },
    CommandHelp {
        name: "render",
        surface: Surface::Both,
        aliases: &["r"],
        usage: "[options] [target...]",
        summary: "Render manifests to manifests/",
        detail: "Defaults to the same scope as apply, so a render previews exactly what\n\
                 an apply would send. Writes manifests/<env>-manifests.mutable.yaml, or\n\
                 -manifests.yaml / -manifests.immutable.yaml as --scope says. With\n\
                 targets the output goes to -manifests.selection.yaml so the full\n\
                 manifest is never overwritten.",
        subcommands: &[],
        options: &[("--scope <scope>", SCOPE_MUTABLE)],
        examples: &[
            ("render", "what a default apply would send"),
            ("render --scope all", "every resource, immutable included"),
            (
                "render deployment/api",
                "one resource to the selection file",
            ),
        ],
        targets: true,
    },
    CommandHelp {
        name: "apply",
        surface: Surface::Both,
        aliases: &["a"],
        usage: "[options] [target...]",
        summary: "Apply manifests via kubectl",
        detail: "Renders the selected resources, then runs `kubectl apply -f` on the\n\
                 rendered file. Waits for each Deployment, StatefulSet and DaemonSet\n\
                 to become ready afterwards, so a rollout that never comes up is\n\
                 reported as a failure rather than a success.",
        subcommands: &[],
        options: &[
            (
                "--replicas <KIND/NAME=N>",
                "Set replicas on a deployment or statefulset; repeatable",
            ),
            ("--except <KIND/NAME>", "Leave a resource out; repeatable"),
            ("--dry-run", "Pass --dry-run=client to kubectl"),
            ("--no-wait", "Return once kubectl accepts the manifests"),
            ("--timeout <seconds>", "Per-rollout wait, default 300"),
            ("--scope <scope>", SCOPE_MUTABLE),
        ],
        examples: &[
            ("apply", "every mutable resource, waiting for rollouts"),
            ("apply deployment/api", "one deployment"),
            ("apply deployment service", "every deployment and service"),
            ("apply --dry-run '*/api'", "preview everything named api"),
            ("apply --no-wait", "do not block on readiness"),
            ("apply --scope all namespace", "include immutable resources"),
        ],
        targets: true,
    },
    CommandHelp {
        name: "diff",
        surface: Surface::Both,
        aliases: &["df"],
        usage: "[options] [target...]",
        summary: "Show what applying would change",
        detail: "Renders the selected resources, then runs `kubectl diff -f` on the\n\
                 rendered file. Shows the change against live cluster state, which a\n\
                 render on its own cannot.",
        subcommands: &[],
        options: &[("--scope <scope>", SCOPE_MUTABLE)],
        examples: &[
            ("diff", "what a default apply would change"),
            ("diff deployment/api", "one deployment"),
        ],
        targets: true,
    },
    CommandHelp {
        name: "prune",
        surface: Surface::Both,
        aliases: &[],
        usage: "[--dry-run]",
        summary: "Remove resources the overlay no longer declares",
        detail: "Lists live resources labelled as belonging to this environment,\n\
                 compares them against what the overlay renders, and deletes the\n\
                 difference. `delete` only removes what the overlay still declares, so\n\
                 without this an entry removed from an overlay would live on in the\n\
                 cluster forever. Namespaces and `raw` resources are never pruned, nor are\n\
                 objects other controllers derived from yours (certificates, endpoints).",
        subcommands: &[],
        options: &[("--dry-run", "List what would be pruned, delete nothing")],
        examples: &[
            ("prune --dry-run", "what has been orphaned"),
            ("prune", "remove it"),
        ],
        targets: false,
    },
    CommandHelp {
        name: "delete",
        surface: Surface::Both,
        aliases: &["d", "del"],
        usage: "[options] [target...]",
        summary: "Delete manifests via kubectl",
        detail: "Renders the selected resources, then runs `kubectl delete -f` on the\n\
                 rendered file. Immutable resources are skipped unless --scope says\n\
                 otherwise.",
        subcommands: &[],
        options: &[("--scope <scope>", SCOPE_MUTABLE)],
        examples: &[
            ("delete", "every mutable resource"),
            ("delete job/migrate", "one job"),
            (
                "delete --scope all namespace/prod",
                "including immutable ones",
            ),
        ],
        targets: true,
    },
    CommandHelp {
        name: "images",
        surface: Surface::Both,
        aliases: &[],
        usage: "[options]",
        summary: "Check overlay image tags for newer registry tags",
        detail: "Scans every deployment*.yaml.j2 template under overlays/ for `image:`\n\
                 lines, and checks each one's registry for a newer tag with the same\n\
                 prefix/suffix and at least as many version components. A floating tag\n\
                 (latest, stable, edge, main, master, dev, nightly) is reported but never\n\
                 compared.",
        subcommands: &[],
        options: &[
            (
                "--update",
                "Rewrite templates in place to the newest tag found",
            ),
            (
                "--registry-auth <REGISTRY=USER:PASS>",
                "Credentials for one registry; repeatable",
            ),
        ],
        examples: &[
            ("images", "list available updates"),
            (
                "images --update",
                "rewrite templates to the newest compatible tag",
            ),
        ],
        targets: false,
    },
    CommandHelp {
        name: "validate",
        surface: Surface::Both,
        aliases: &[],
        usage: "[options] [target...]",
        summary: "Check custom resources against their CRD schemas",
        detail: "Renders the environment in memory and checks every resource whose kind has\n\
                 a schema held, the way the API server will: an unknown or misspelt field is\n\
                 an error (with a suggestion), as are wrong types, missing required fields\n\
                 and values outside an enum. cert-manager, Traefik and Gateway API schemas\n\
                 ship in the binary; `schemas fetch` adds more. Kinds with no schema are\n\
                 counted, not failed. Nothing is sent to a cluster, and CEL rules a CRD carries\n\
                 are not evaluated. Exits non-zero if anything failed.",
        subcommands: &[],
        options: &[
            ("--scope <scope>", SCOPE_ALL),
            (
                "--file <file>",
                "CLI only: check files instead (`-` is stdin)",
            ),
        ],
        examples: &[
            ("validate", "every resource of the environment"),
            ("validate certificate", "just the certificates"),
        ],
        targets: true,
    },
    CommandHelp {
        name: "secrets",
        surface: Surface::Cli,
        aliases: &[],
        usage: "<keygen|set|import|list|show|remove|rekey|sync> ...",
        summary: "Keep secrets in git, encrypted (age), and open them at install",
        detail: "An overlay's secrets.yaml holds NAME: ENC[age,...] - names readable, each value\n\
                 encrypted on its own - and is committed and packed like any other file. The\n\
                 key that opens it is not: RIVETER_AGE_KEY, or RIVETER_AGE_KEY_FILE (a file,\n\
                 which can be mounted wherever riveter runs), else ~/.config/riveter/age.key.\n\
                 At install the values join the others, below --env-file and --set. `sync` is the\n\
                 older route: it writes a .env to a Secret for an install to mount.",
        subcommands: &[
            ("keygen", "Make a key pair; prints the public recipient"),
            (
                "set",
                "Encrypt one value (from --value or stdin) into a file",
            ),
            (
                "import",
                "Move values from a dotenv file into a file, encrypted",
            ),
            ("list", "The names in a file, never the values"),
            ("show", "Print one value decrypted"),
            ("remove", "Drop a name from a file"),
            ("rekey", "Encrypt to different recipients"),
            ("sync", "Write .env files to Secrets in the cluster (older)"),
        ],
        options: &[
            ("--recipient <age1...>", "Who a new file encrypts to"),
            ("--out <file>", "keygen: where the private key goes"),
            ("--from <dotenv>", "import: the file to read"),
            ("--all", "sync: every overlay that has a .env"),
            (
                "--namespace <ns>",
                "sync: where the Secrets go (default forge)",
            ),
            ("--dry-run", "sync: report what would be synced"),
        ],
        examples: &[
            ("secrets keygen --out ~/.config/riveter/age.key", "once"),
            (
                "secrets import overlays/media/secrets.yaml --from overlays/media/.env",
                "move a .env in",
            ),
            (
                "secrets set overlays/media/secrets.yaml DB_PASSWORD",
                "value from stdin",
            ),
        ],
        targets: false,
    },
    CommandHelp {
        name: "schemas",
        surface: Surface::Cli,
        aliases: &[],
        usage: "<list|fetch>",
        summary: "Manage the CRD schemas validate uses",
        detail: "Schemas are read from a cluster's CRDs once and kept in a cache directory\n\
                 ($RIVETER_SCHEMA_DIR, else ~/.cache/riveter/schemas), where they take\n\
                 precedence over the ones in the binary. After upgrading a CRD, fetch again.",
        subcommands: &[
            ("list", "What is held, and from where"),
            ("fetch", "Read schemas from a cluster"),
        ],
        options: &[
            ("--crd <name>", "fetch: one CRD; repeatable"),
            ("--all", "fetch: every CRD in the cluster"),
            (
                "--context <ctx>",
                "fetch: a kubectl context other than the current",
            ),
            ("--output <dir>", "fetch: write here instead of the cache"),
        ],
        examples: &[
            ("schemas fetch", "refresh the kinds riveter templates"),
            ("schemas fetch --all", "every CRD, for raw resources"),
        ],
        targets: false,
    },
    CommandHelp {
        name: "pack",
        surface: Surface::Cli,
        aliases: &[],
        usage: "[options]",
        summary: "Build the environment into a .rivet package",
        detail: "A package is one whole overlay directory: its overlay.yaml and everything\n\
                 it includes, with every image tag pinned to a digest, as a checksummed\n\
                 tar.zst. The overlay carries a rivet.toml naming the package (it must match\n\
                 the directory) and giving its version. Dotfiles - .env above all - are\n\
                 never packed. Pinning needs registry access; --no-pin skips it.",
        subcommands: &[],
        options: &[
            (
                "--version-suffix <suffix>",
                "Append +<suffix> to the version",
            ),
            ("--no-pin", "Leave image tags as written"),
            (
                "--out <dir>",
                "Where to write the package (default packages)",
            ),
            (
                "--registry-auth <REGISTRY=USER:PASS>",
                "Credentials for digest lookups; repeatable",
            ),
        ],
        examples: &[
            ("pack", "packages/<env>-<version>.rivet"),
            ("pack --version-suffix b123", "a CI build: 0.4.0+b123"),
            (
                "pack --version-suffix {timestamp}.{sha}",
                "0.4.0+20261002135500.1a2b3c4, which sorts by time",
            ),
        ],
        targets: false,
    },
    CommandHelp {
        name: "publish",
        surface: Surface::Cli,
        aliases: &[],
        usage: "[options] [file]",
        summary: "Publish a package to Warehouse",
        detail: "Uploads a .rivet - the given file, or the current environment packed first.\n\
                 A version can be published once. Needs RIVETER_WAREHOUSE_URL and either\n\
                 RIVETER_WAREHOUSE_TOKEN or RIVETER_GATEHOUSE_URL with RIVETER_CLIENT_ID and\n\
                 RIVETER_CLIENT_SECRET, and the warehouse:write grant.",
        subcommands: &[],
        options: &[
            (
                "--version-suffix <suffix>",
                "When packing: append +<suffix>",
            ),
            ("--no-pin", "When packing: leave image tags as written"),
            ("--out <dir>", "When packing: where to write the package"),
        ],
        examples: &[
            ("publish", "pack the environment and upload it"),
            (
                "publish packages/forge-0.4.0.rivet",
                "upload an existing file",
            ),
        ],
        targets: false,
    },
    CommandHelp {
        name: "pull",
        surface: Surface::Cli,
        aliases: &[],
        usage: "[options] <package>",
        summary: "Download a package and verify it",
        detail: "Fetches name, name@version or name@latest from Warehouse, checks its digest\n\
                 against the registry's record and validates it before saving it.",
        subcommands: &[],
        options: &[("--out <dir>", "Where to save it (default packages)")],
        examples: &[("pull forge@0.4.0", "packages/forge-0.4.0.rivet")],
        targets: false,
    },
    CommandHelp {
        name: "install",
        surface: Surface::Cli,
        aliases: &["i"],
        usage: "[options] <package> [target...]",
        summary: "Fetch a package, render it and apply it",
        detail: "The package is name, name@version, or the path of a .rivet file. It is\n\
                 unpacked to a scratch directory and rendered and applied exactly as `apply`\n\
                 would, with each resource labelled riveter.forge/package and annotated with\n\
                 its version. Variables come from the package's values.toml, then --env-file,\n\
                 then --set; the working directory's .env is not read.",
        subcommands: &[],
        options: &[
            ("--env-file <file>", "Variables in dotenv format"),
            (
                "--set <KEY=VALUE>",
                "One variable; repeatable, wins over all",
            ),
            ("--dry-run", "Pass --dry-run=client to kubectl"),
            ("--no-wait", "Return once kubectl accepts the manifests"),
            ("--timeout <seconds>", "Per-rollout wait, default 300"),
            ("--scope <scope>", SCOPE_MUTABLE),
        ],
        examples: &[
            (
                "install forge --env-file overlays/forge/.env",
                "the newest forge",
            ),
            (
                "install forge@0.4.0 --dry-run",
                "what that version would apply",
            ),
            ("install ./forge-0.4.0.rivet", "a local file"),
        ],
        targets: true,
    },
    CommandHelp {
        name: "remote",
        surface: Surface::Cli,
        aliases: &[],
        usage: "<list|versions>",
        summary: "Browse the packages in Warehouse",
        detail: "Needs the same RIVETER_WAREHOUSE_* settings as publish.",
        subcommands: &[
            ("list", "The newest version of every package"),
            ("versions <name>", "Every version of one package"),
        ],
        options: &[],
        examples: &[("remote versions forge", "all published forge versions")],
        targets: false,
    },
    CommandHelp {
        name: "help",
        surface: Surface::Both,
        aliases: &["h"],
        usage: "[command]",
        summary: "Show help, or detail for one command",
        detail: "",
        subcommands: &[],
        options: &[],
        examples: &[("help apply", "everything `apply` accepts")],
        targets: false,
    },
    CommandHelp {
        name: "repl",
        surface: Surface::Cli,
        aliases: &[],
        usage: "",
        summary: "Start the interactive REPL",
        detail: "Also what `riveter` does with no arguments.",
        subcommands: &[],
        options: &[],
        examples: &[],
        targets: false,
    },
    CommandHelp {
        name: "exit",
        surface: Surface::Repl,
        aliases: &["quit", "q"],
        usage: "",
        summary: "Leave the REPL",
        detail: "",
        subcommands: &[],
        options: &[],
        examples: &[],
        targets: false,
    },
];

#[must_use]
pub fn find(name: &str) -> Option<&'static CommandHelp> {
    COMMANDS
        .iter()
        .find(|c| c.name == name || c.aliases.contains(&name))
}

/// Like [`find`], but only for commands that exist on the given surface.
#[must_use]
pub fn find_on(name: &str, on: Surface) -> Option<&'static CommandHelp> {
    find(name).filter(|c| c.surface.shows(on))
}

/// The command tree plus the shared Targets and Scopes reference — what the
/// REPL's bare `help` prints.
#[must_use]
pub fn overview() -> String {
    format!("{}\n{}", command_tree(Surface::Repl), reference())
}

fn visible(on: Surface) -> impl Iterator<Item = &'static CommandHelp> {
    COMMANDS.iter().filter(move |c| c.surface.shows(on))
}

/// The Targets and Scopes sections, shared by every command that uses them.
#[must_use]
pub fn reference() -> String {
    format!("Targets:\n{}\n\n{SCOPES}\n", indent(TARGETS, 2))
}

/// The full command tree: every command with its subcommands and options
/// indented beneath it.
#[must_use]
pub fn command_tree(on: Surface) -> String {
    // Widest of the command signatures and of the rows nested two spaces deeper.
    let width = visible(on)
        .flat_map(|c| {
            std::iter::once(signature(c).len())
                .chain(c.subcommands.iter().map(|(s, _)| s.len() + 2))
                .chain(c.options.iter().map(|(o, _)| o.len() + 2))
        })
        .max()
        .unwrap_or(24);

    let mut out = String::from("Commands:\n");

    for cmd in visible(on) {
        let _ = writeln!(out, "\n  {:<width$}  {}", signature(cmd), cmd.summary);

        for (name, about) in cmd.subcommands {
            let _ = writeln!(out, "    {:<w$}  {about}", name, w = width - 2);
        }
        for (flag, about) in cmd.options {
            let _ = writeln!(out, "    {:<w$}  {about}", flag, w = width - 2);
        }
    }

    out
}

/// Detail for a single command, as shown by `help <command>`.
#[must_use]
pub fn detail(cmd: &CommandHelp) -> String {
    let mut out = format!("{}\n", signature(cmd));

    if !cmd.detail.is_empty() {
        let _ = write!(out, "\n{}\n", indent(cmd.detail, 2));
    }

    if !cmd.subcommands.is_empty() {
        out.push_str("\nSubcommands:\n");
        let width = max_len(cmd.subcommands);
        for (name, about) in cmd.subcommands {
            let _ = writeln!(out, "  {name:<width$}  {about}");
        }
    }

    if !cmd.options.is_empty() {
        out.push_str("\nOptions:\n");
        let width = max_len(cmd.options);
        for (flag, about) in cmd.options {
            let _ = writeln!(out, "  {flag:<width$}  {about}");
        }
        if cmd.options.iter().any(|(f, _)| f.starts_with("--scope")) {
            let _ = write!(out, "\n{SCOPES}\n");
        }
    }

    if cmd.targets {
        let _ = write!(out, "\nTargets:\n{}\n", indent(TARGETS, 2));
    }

    if !cmd.examples.is_empty() {
        out.push_str("\nExamples:\n");
        let width = max_len(cmd.examples);
        for (example, about) in cmd.examples {
            let _ = writeln!(out, "  {example:<width$}  {about}");
        }
    }

    out
}

/// The `targets` topic, indented to match every other help block.
#[must_use]
pub fn targets() -> String {
    format!("Targets:\n{}", indent(TARGETS, 2))
}

#[must_use]
pub fn unknown_topic(name: &str, on: Surface) -> String {
    format!(
        "no help for `{name}`. topics: {}, targets",
        visible(on).map(|c| c.name).collect::<Vec<_>>().join(", ")
    )
}

/// `delete | d | del [--scope <scope>] [target...]`
fn signature(cmd: &CommandHelp) -> String {
    let mut sig = String::from(cmd.name);
    for alias in cmd.aliases {
        sig.push_str(" | ");
        sig.push_str(alias);
    }
    if !cmd.usage.is_empty() {
        sig.push(' ');
        sig.push_str(cmd.usage);
    }
    sig
}

fn max_len(rows: &[(&str, &str)]) -> usize {
    rows.iter().map(|(left, _)| left.len()).max().unwrap_or(0)
}

fn indent(text: &str, by: usize) -> String {
    let pad = " ".repeat(by);
    text.lines()
        .map(|line| {
            if line.is_empty() {
                String::new()
            } else {
                format!("{pad}{line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}
