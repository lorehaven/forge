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
        matches!(self, Self::Both) || matches!((self, on), (Self::Cli, Self::Cli) | (Self::Repl, Self::Repl))
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
        detail: "Writes manifests/<env>-manifests.yaml, or -manifests.mutable.yaml /\n\
                 -manifests.immutable.yaml when --scope narrows it. With targets the\n\
                 output goes to -manifests.selection.yaml so the full manifest is\n\
                 never overwritten.",
        subcommands: &[],
        options: &[("--scope <scope>", SCOPE_ALL)],
        examples: &[
            ("render", "the whole environment"),
            ("render deployment/api", "one resource to the selection file"),
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
                 rendered file.",
        subcommands: &[],
        options: &[
            ("--dry-run", "Pass --dry-run=client to kubectl"),
            ("--scope <scope>", SCOPE_MUTABLE),
        ],
        examples: &[
            ("apply", "every mutable resource"),
            ("apply deployment/api", "one deployment"),
            ("apply deployment service", "every deployment and service"),
            ("apply --dry-run '*/api'", "preview everything named api"),
            ("apply --scope all namespace", "include immutable resources"),
        ],
        targets: true,
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
            ("delete --scope all namespace/prod", "including immutable ones"),
        ],
        targets: true,
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
