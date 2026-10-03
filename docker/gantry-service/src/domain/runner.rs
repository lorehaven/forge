//! The loop the runner Job executes: validate the plan, run each step's commands, stop at the first
//! failure, undo what has an undo. Commands go through `Exec`, so the tests record them instead of
//! running them.

use crate::domain::commands::{Command, Context};
use crate::domain::steps::{Plan, Step};
use std::io::Write;
use std::time::Duration;

/// The last line of the runner's output, which is how the service learns the outcome from a Job's log
/// (a Job only reports success or failure, and "rolled back" is a third thing).
pub const RESULT_MARKER: &str = "GANTRY-RESULT:";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Succeeded,
    Failed(String),
    /// A step failed and its undo ran.
    RolledBack(String),
}

impl Outcome {
    pub fn marker(&self) -> String {
        match self {
            Self::Succeeded => format!("{RESULT_MARKER} ok"),
            Self::Failed(reason) => format!("{RESULT_MARKER} failed {reason}"),
            Self::RolledBack(reason) => format!("{RESULT_MARKER} rolled_back {reason}"),
        }
    }
}

/// What the service reads back out of a finished runner's log.
pub fn parse_outcome(log: &str) -> Option<Outcome> {
    let line = log
        .lines()
        .rev()
        .find_map(|line| line.strip_prefix(RESULT_MARKER))?
        .trim();
    let (state, reason) = line.split_once(' ').unwrap_or((line, ""));
    match state {
        "ok" => Some(Outcome::Succeeded),
        "failed" => Some(Outcome::Failed(reason.to_string())),
        "rolled_back" => Some(Outcome::RolledBack(reason.to_string())),
        _ => None,
    }
}

pub trait Exec {
    /// The command's exit code; `Err` if it could not be started at all.
    fn run(&mut self, command: &Command) -> std::io::Result<i32>;
    /// Exit code and standard output, for commands whose answer is read.
    fn capture(&mut self, command: &Command) -> std::io::Result<(i32, String)>;
    /// Waiting between polls; a test makes this instant.
    fn sleep(&mut self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

/// How often a wait looks again.
const POLL: Duration = Duration::from_secs(2);

pub fn run(plan: &Plan, ctx: &Context, exec: &mut dyn Exec, out: &mut dyn Write) -> Outcome {
    let outcome = execute(plan, ctx, exec, out);
    let _ = writeln!(out, "{}", outcome.marker());
    outcome
}

fn execute(plan: &Plan, ctx: &Context, exec: &mut dyn Exec, out: &mut dyn Write) -> Outcome {
    // The service validated this already; checking again means a tampered Job spec still cannot reach
    // outside the allow-list.
    if let Err(reason) = plan.validate(&ctx.allowed_namespaces) {
        return Outcome::Failed(reason);
    }

    let total = plan.steps.len();
    for (index, step) in plan.steps.iter().enumerate() {
        let _ = writeln!(out, "== step {}/{total}: {}", index + 1, step.describe());

        // The values Secret is mounted as optional: a package whose variables all have defaults has none.
        // If it is needed after all, the install says which variable is missing, before anything changes.
        let step = &match step.values_secret() {
            Some(secret) if !ctx.values_dir.join(secret).join("env").is_file() => {
                let _ = writeln!(
                    out,
                    "(no values in {secret}: using the package's own defaults)"
                );
                step.without_values()
            }
            _ => step.clone(),
        };

        if let Step::ApplyYaml { yaml, .. } = step {
            // The file kubectl applies is written first, from the plan; nothing else is applied from it.
            let path = crate::domain::commands::edit_file(&ctx.packages_dir, yaml);
            if let Err(error) = std::fs::create_dir_all(&ctx.packages_dir)
                .and_then(|()| std::fs::write(&path, yaml))
            {
                let reason = format!(
                    "step {} ({}): could not write {}: {error}",
                    index + 1,
                    step.describe(),
                    path.display()
                );
                let _ = writeln!(out, "!! {reason}");
                return Outcome::Failed(reason);
            }
        }

        if let Step::RestartUsers {
            namespace,
            kind,
            name,
            timeout_secs,
        } = step
        {
            if let Err(failure) =
                restart_users(ctx, namespace, kind, name, *timeout_secs, exec, out)
            {
                let reason = format!("step {} ({}): {failure}", index + 1, step.describe());
                let _ = writeln!(out, "!! {reason}");
                return Outcome::Failed(reason);
            }
            continue;
        }

        if let Step::WaitGone { timeout_secs, .. } = step {
            if let Err(failure) = wait_gone(&step.commands(ctx), *timeout_secs, exec, out) {
                let reason = format!("step {} ({}): {failure}", index + 1, step.describe());
                let _ = writeln!(out, "!! {reason}");
                return Outcome::Failed(reason);
            }
            continue;
        }

        for command in step.commands(ctx) {
            let _ = writeln!(out, "$ {}", command.display());
            if ctx.dry_run && command.mutating {
                let _ = writeln!(out, "(dry run: not executed)");
                continue;
            }

            let failure = match exec.run(&command) {
                Ok(0) => continue,
                Ok(code) => format!("`{}` exited with {code}", command.display()),
                Err(error) => format!("could not start `{}`: {error}", command.program),
            };
            let reason = format!("step {} ({}): {failure}", index + 1, step.describe());
            let _ = writeln!(out, "!! {reason}");

            let Some(undo) = step.rollback() else {
                return Outcome::Failed(reason);
            };
            let _ = writeln!(out, "== rolling back: {}", undo.display());
            return match exec.run(&undo) {
                Ok(0) => Outcome::RolledBack(reason),
                Ok(code) => Outcome::Failed(format!("{reason}; the rollback also failed ({code})")),
                Err(error) => {
                    Outcome::Failed(format!("{reason}; the rollback could not start: {error}"))
                }
            };
        }
    }
    Outcome::Succeeded
}

/// Restarts every workload in `namespace` that uses the ConfigMap or Secret, and waits for each.
///
/// "Uses" is read from the workloads themselves: an env var, `envFrom` or a volume that names it.
/// Restarting is the only way a running pod sees a changed value that arrives as an environment variable,
/// and for a mounted file it is the only way that is certain to be picked up everywhere.
fn restart_users(
    ctx: &Context,
    namespace: &str,
    kind: &str,
    name: &str,
    timeout_secs: u64,
    exec: &mut dyn Exec,
    out: &mut dyn Write,
) -> Result<(), String> {
    let step = Step::RestartUsers {
        namespace: namespace.to_string(),
        kind: kind.to_string(),
        name: name.to_string(),
        timeout_secs,
    };
    let listing = &step.commands(ctx)[0];
    let _ = writeln!(out, "$ {}", listing.display());
    let (code, stdout) = exec
        .capture(listing)
        .map_err(|e| format!("could not start `{}`: {e}", listing.program))?;
    if code != 0 {
        return Err(format!("`{}` exited with {code}", listing.display()));
    }
    let document: serde_json::Value =
        serde_json::from_str(&stdout).map_err(|e| format!("could not read the workloads: {e}"))?;

    let users: Vec<(String, String)> = document["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|item| uses(&item["spec"]["template"]["spec"], kind, name))
        .filter_map(|item| {
            Some((
                item["kind"].as_str()?.to_ascii_lowercase(),
                item["metadata"]["name"].as_str()?.to_string(),
            ))
        })
        .collect();

    if users.is_empty() {
        let _ = writeln!(out, "nothing in {namespace} uses {kind}/{name}");
        return Ok(());
    }
    for (workload_kind, workload) in users {
        for args in [vec!["rollout", "restart"], vec!["rollout", "status"]] {
            let mut full = vec!["-n".to_string(), namespace.to_string()];
            full.extend(args.iter().map(ToString::to_string));
            full.push(format!("{workload_kind}/{workload}"));
            if args[1] == "status" {
                full.push(format!("--timeout={timeout_secs}s"));
            }
            let command = Command {
                program: "kubectl".to_string(),
                args: full,
                mutating: args[1] == "restart",
            };
            let _ = writeln!(out, "$ {}", command.display());
            if ctx.dry_run && command.mutating {
                let _ = writeln!(out, "(dry run: not executed)");
                continue;
            }
            match exec.run(&command) {
                Ok(0) => {}
                Ok(code) => return Err(format!("`{}` exited with {code}", command.display())),
                Err(error) => {
                    return Err(format!("could not start `{}`: {error}", command.program));
                }
            }
        }
    }
    Ok(())
}

/// Whether a pod spec mentions the ConfigMap or Secret by name: env, envFrom, or a volume.
fn uses(pod_spec: &serde_json::Value, kind: &str, name: &str) -> bool {
    let (ref_key, from_key, volume_key, volume_name_key) = if kind == "secret" {
        ("secretKeyRef", "secretRef", "secret", "secretName")
    } else {
        ("configMapKeyRef", "configMapRef", "configMap", "name")
    };
    let named = |value: &serde_json::Value| value["name"].as_str() == Some(name);

    let in_volumes = pod_spec["volumes"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|v| v[volume_key][volume_name_key].as_str() == Some(name));
    let in_containers = ["containers", "initContainers"].iter().any(|field| {
        pod_spec[field]
            .as_array()
            .into_iter()
            .flatten()
            .any(|container| {
                container["envFrom"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|e| named(&e[from_key]))
                    || container["env"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .any(|e| named(&e["valueFrom"][ref_key]))
            })
    });
    in_volumes || in_containers
}

/// Lists pods until none are left. `kubectl wait --for=delete` cannot do this: it errors when nothing
/// matches, and nothing matching is the state being waited for.
fn wait_gone(
    listings: &[Command],
    timeout_secs: u64,
    exec: &mut dyn Exec,
    out: &mut dyn Write,
) -> Result<(), String> {
    let mut waited = Duration::ZERO;
    loop {
        let mut remaining = Vec::new();
        for listing in listings {
            let (code, stdout) = exec
                .capture(listing)
                .map_err(|e| format!("could not start `{}`: {e}", listing.program))?;
            if code != 0 {
                return Err(format!("`{}` exited with {code}", listing.display()));
            }
            remaining.extend(
                stdout
                    .lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .map(str::to_string),
            );
        }
        if remaining.is_empty() {
            let _ = writeln!(out, "none left");
            return Ok(());
        }
        if waited >= Duration::from_secs(timeout_secs) {
            return Err(format!(
                "still there after {timeout_secs}s: {}",
                remaining.join(", ")
            ));
        }
        let _ = writeln!(
            out,
            "waiting for {} pod(s): {}",
            remaining.len(),
            remaining.join(", ")
        );
        exec.sleep(POLL);
        waited += POLL;
    }
}

/// Runs commands as real processes, their output going to ours (which is the Job's log).
pub struct ProcessExec;

impl Exec for ProcessExec {
    fn run(&mut self, command: &Command) -> std::io::Result<i32> {
        let status = std::process::Command::new(&command.program)
            .args(&command.args)
            .status()?;
        Ok(status.code().unwrap_or(-1))
    }

    fn capture(&mut self, command: &Command) -> std::io::Result<(i32, String)> {
        let output = std::process::Command::new(&command.program)
            .args(&command.args)
            .stderr(std::process::Stdio::inherit())
            .output()?;
        Ok((
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stdout).into_owned(),
        ))
    }
}
