//! A step as the exact commands the runner executes. Pure (no process is started here), so what a plan
//! will do is testable - and printable in a dry run - without a cluster.

use crate::domain::steps::Step;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Command {
    pub program: String,
    pub args: Vec<String>,
    /// Changes the cluster. A dry run prints these instead of running them.
    pub mutating: bool,
}

impl Command {
    fn new(program: &str, args: Vec<String>, mutating: bool) -> Self {
        Self {
            program: program.to_string(),
            args,
            mutating,
        }
    }

    /// As a shell would show it. For the log only; it is never run through a shell.
    pub fn display(&self) -> String {
        std::iter::once(self.program.as_str())
            .chain(self.args.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Where the runner finds things.
#[derive(Clone, Debug)]
pub struct Context {
    /// `<values_dir>/<secret>/env` is a package's variables (a mounted Secret, or a file locally).
    pub values_dir: PathBuf,
    /// Downloaded packages: `<packages_dir>/<name>-<version>.rivet`.
    pub packages_dir: PathBuf,
    /// Namespaces this runner may touch; empty means any.
    pub allowed_namespaces: Vec<String>,
    pub dry_run: bool,
    /// A directory of `.rivet` files to take packages from instead of Warehouse (a local run with no
    /// registry): a pull is then a copy.
    pub source_dir: Option<PathBuf>,
}

/// Where an edit's YAML is written for `kubectl apply`: named by its content, so the same edit is the
/// same file.
#[must_use]
pub fn edit_file(dir: &Path, yaml: &str) -> PathBuf {
    let digest = rivet_package::sha256_hex(yaml.as_bytes());
    dir.join(format!("edit-{}.yaml", &digest[..12]))
}

#[must_use]
pub fn package_file(packages_dir: &Path, package: &str, version: &str) -> PathBuf {
    packages_dir.join(format!("{package}-{version}.rivet"))
}

impl Step {
    #[must_use]
    pub fn commands(&self, ctx: &Context) -> Vec<Command> {
        match self {
            Self::Pull { package, version } => pull_commands(ctx, package, version),
            Self::Check {
                package,
                version,
                values_secret,
            } => check_commands(ctx, package, version, values_secret.as_deref()),
            Self::Install {
                package,
                version,
                sets,
                replicas,
                values_secret,
                timeout_secs,
                targets,
                except,
                no_wait,
                ..
            } => {
                let mut args = install_args(ctx, package, version, values_secret.as_deref());
                // The package's own record of itself, whatever part of it this applies.
                args.push("--inventory".into());
                push_overrides(&mut args, sets, replicas, *timeout_secs);
                push_selection(&mut args, ctx, except, *no_wait, targets);
                vec![Command::new("riveter", args, !ctx.dry_run)]
            }
            Self::Scale {
                namespace,
                kind,
                name,
                replicas,
            } => scale_commands(namespace, kind, name, *replicas),
            Self::Delete {
                resource,
                namespace,
            } => delete_commands(resource, namespace.as_deref()),
            Self::ApplyYaml { yaml, .. } => vec![Command::new(
                "kubectl",
                vec![
                    "apply".to_string(),
                    "-f".into(),
                    edit_file(&ctx.packages_dir, yaml).display().to_string(),
                ],
                true,
            )],
            // Executed by the runner, which has to read what uses it (see `runner::restart_users`).
            Self::RestartUsers { namespace, .. } => vec![Command::new(
                "kubectl",
                namespaced(
                    namespace,
                    ["get", "deployments,statefulsets,daemonsets", "-o", "json"],
                ),
                false,
            )],
            Self::DeletePods {
                selector,
                namespaces,
            } => per_scope(
                ctx,
                namespaces,
                ["delete", "pods", "-l", selector, "--wait=false"],
                true,
            ),
            // Executed by the runner as a poll (see `runner::wait_gone`), because `kubectl wait` fails when
            // nothing matches - and "nothing matches" is exactly the state being waited for.
            Self::WaitGone {
                selector,
                namespaces,
                ..
            } => per_scope(
                ctx,
                namespaces,
                ["get", "pods", "-l", selector, "-o", "name"],
                false,
            ),
            Self::Rollout {
                namespace,
                kind,
                name,
                timeout_secs,
                ..
            } => vec![Command::new(
                "kubectl",
                namespaced(
                    namespace,
                    [
                        "rollout",
                        "status",
                        &format!("{kind}/{name}"),
                        &format!("--timeout={timeout_secs}s"),
                    ],
                ),
                false,
            )],
        }
    }

    /// What undoes a failed step, for the steps that have one.
    #[must_use]
    pub fn rollback(&self) -> Option<Command> {
        match self {
            Self::Rollout {
                namespace,
                kind,
                name,
                rollback_on_failure: true,
                ..
            } => Some(Command::new(
                "kubectl",
                namespaced(namespace, ["rollout", "undo", &format!("{kind}/{name}")]),
                true,
            )),
            _ => None,
        }
    }
}

/// A package into `packages_dir`: copied from a local source directory when there is one, else pulled
/// from Warehouse.
/// Renders the package with its variables and dry-runs it, reaching no cluster.
fn check_commands(
    ctx: &Context,
    package: &str,
    version: &str,
    values_secret: Option<&str>,
) -> Vec<Command> {
    let mut args = install_args(ctx, package, version, values_secret);
    args.push("--inventory".into());
    args.push("--dry-run".into());
    vec![Command::new("riveter", args, false)]
}

/// `kubectl scale` of one workload to a replica count.
fn scale_commands(namespace: &str, kind: &str, name: &str, replicas: u32) -> Vec<Command> {
    let target = format!("{kind}/{name}");
    let count = format!("--replicas={replicas}");
    vec![Command::new(
        "kubectl",
        namespaced(namespace, ["scale", &target, &count]),
        true,
    )]
}

/// Deletes one resource of any kind; a missing one is not an error.
fn delete_commands(resource: &str, namespace: Option<&str>) -> Vec<Command> {
    let mut args = vec![
        "delete".to_string(),
        resource.to_string(),
        "--ignore-not-found".into(),
        "--timeout=120s".into(),
    ];
    if let Some(namespace) = namespace {
        args.extend(["-n".to_string(), namespace.to_string()]);
    }
    vec![Command::new("kubectl", args, true)]
}

fn pull_commands(ctx: &Context, package: &str, version: &str) -> Vec<Command> {
    if let Some(source) = ctx.source_dir.as_deref() {
        return vec![Command::new(
            "cp",
            vec![
                package_file(source, package, version).display().to_string(),
                ctx.packages_dir.display().to_string(),
            ],
            false,
        )];
    }
    vec![Command::new(
        "riveter",
        vec![
            "pull".into(),
            format!("{package}@{version}"),
            "--out".into(),
            ctx.packages_dir.display().to_string(),
        ],
        false,
    )]
}

/// `kubectl -n <namespace> <rest...>`.
fn namespaced<const N: usize>(namespace: &str, rest: [&str; N]) -> Vec<String> {
    ["-n", namespace]
        .into_iter()
        .chain(rest)
        .map(str::to_string)
        .collect()
}

/// One `kubectl <base...> <scope>` per namespace scope the runner may touch.
fn per_scope<const N: usize>(
    ctx: &Context,
    namespaces: &[String],
    base: [&str; N],
    mutating: bool,
) -> Vec<Command> {
    scopes(ctx, namespaces)
        .into_iter()
        .map(|scope| {
            let mut args: Vec<String> = base.iter().map(ToString::to_string).collect();
            args.extend(scope);
            Command::new("kubectl", args, mutating)
        })
        .collect()
}

/// `--set`, `--replicas` and `--timeout`: what an install changes about the package it applies.
fn push_overrides(
    args: &mut Vec<String>,
    sets: &BTreeMap<String, String>,
    replicas: &BTreeMap<String, u32>,
    timeout_secs: Option<u64>,
) {
    for (key, value) in sets {
        args.push("--set".into());
        args.push(format!("{key}={value}"));
    }
    for (resource, count) in replicas {
        args.push("--replicas".into());
        args.push(format!("{resource}={count}"));
    }
    if let Some(seconds) = timeout_secs {
        args.push("--timeout".into());
        args.push(seconds.to_string());
    }
}

/// Which resources an install applies, and whether it waits for them.
fn push_selection(
    args: &mut Vec<String>,
    ctx: &Context,
    except: &[String],
    no_wait: bool,
    targets: &[String],
) {
    // Riveter's own client-side dry run reaches no cluster, so it is safe to actually run.
    for resource in except {
        args.push("--except".into());
        args.push(resource.clone());
    }
    if no_wait {
        args.push("--no-wait".into());
    }
    if ctx.dry_run {
        args.push("--dry-run".into());
    }
    // Positional: only these resources of the package.
    args.extend(targets.iter().cloned());
}

fn install_args(ctx: &Context, package: &str, version: &str, secret: Option<&str>) -> Vec<String> {
    let mut args = vec![
        "install".to_string(),
        package_file(&ctx.packages_dir, package, version)
            .display()
            .to_string(),
        // A package is installed whole: ServiceAccounts, Ingresses and the namespace are marked immutable
        // for a routine `apply`, and an install that skipped them would leave them unlabelled (so shown
        // missing here) and never roll out a change to them. Explicit, so a riveter that defaults
        // otherwise cannot change what an operation does.
        "--scope".to_string(),
        "all".to_string(),
    ];
    if let Some(secret) = secret {
        args.push("--env-file".into());
        args.push(
            ctx.values_dir
                .join(secret)
                .join("env")
                .display()
                .to_string(),
        );
    }
    args
}

/// `-n a`, `-n b` ... or `-A` when nothing narrows it; goes after the subcommand, since `-A` is not a global flag. An empty step list falls back to the allow-list.
fn scopes(ctx: &Context, namespaces: &[String]) -> Vec<Vec<String>> {
    let chosen: &[String] = if namespaces.is_empty() {
        &ctx.allowed_namespaces
    } else {
        namespaces
    };
    if chosen.is_empty() {
        return vec![vec!["-A".into()]];
    }
    chosen
        .iter()
        .map(|namespace| vec!["-n".into(), namespace.clone()])
        .collect()
}
