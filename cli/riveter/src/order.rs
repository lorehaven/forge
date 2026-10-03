//! The order an overlay is applied in, and where it stops to wait.
//!
//! An overlay used to be applied in the order it was written, with "order matters" as a comment. Two optional
//! keys on a resource make that declared instead:
//!
//! - **`depends_on: [kind/name, ...]`** - the resource is applied after those. [`order_by_dependencies`]
//!   reorders the overlay once, right after it is rendered, so `list`, `render`, `apply` and `diff` all see
//!   the same order. An overlay that uses none of it is not reordered at all.
//! - **`wait: complete | ready`** (with `wait_timeout: <seconds>`) - an apply stops after this resource until
//!   it is done (a `Job` has completed) or ready (a `Deployment`, `StatefulSet` or `DaemonSet` has rolled
//!   out), and only then goes on. This is what lets a migration finish before the services that need its
//!   schema start.

use crate::render::{ResourceRef, kinds_match, resource_refs};
use anyhow::{Context as _, Result, bail, ensure};
use serde_yaml::Value as YamlValue;

/// What an apply waits for after applying a resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitFor {
    /// A `Job` has completed (and fails the apply if it fails instead).
    Complete,
    /// A `Deployment`, `StatefulSet` or `DaemonSet` has rolled out.
    Ready,
}

impl WaitFor {
    /// The word the overlay writes.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Ready => "ready",
        }
    }
}

/// A point an apply waits at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gate {
    /// The resource waited on.
    pub resource: ResourceRef,
    /// For what.
    pub wait: WaitFor,
    /// Overrides the apply's `--timeout` for this gate, in seconds.
    pub timeout: Option<u64>,
}

/// Some resources applied together, then (if there is one) a gate waited on before the next phase.
#[derive(Debug, Clone)]
pub struct Phase {
    /// The manifest holding just this phase's resources.
    pub path: String,
    /// What it holds, in order.
    pub resources: Vec<ResourceRef>,
    /// What to wait for once it has been applied. Only the last phase of an apply has none.
    pub gate: Option<Gate>,
}

/// The `kind/name` entries of a resource's `depends_on`, validated for shape.
fn dependency_entries(res: &YamlValue, who: &ResourceRef) -> Result<Vec<(String, String, String)>> {
    let value = &res["depends_on"];
    if value.is_null() {
        return Ok(Vec::new());
    }
    let list = value
        .as_sequence()
        .with_context(|| format!("{who}: depends_on must be a list of `kind/name`"))?;

    list.iter()
        .map(|entry| {
            let text = entry
                .as_str()
                .with_context(|| format!("{who}: depends_on entries are `kind/name` strings"))?;
            let (kind, name) = text
                .split_once('/')
                .filter(|(k, n)| !k.trim().is_empty() && !n.trim().is_empty())
                .with_context(|| format!("{who}: depends_on entry `{text}` is not `kind/name`"))?;
            Ok((
                text.to_string(),
                kind.trim().to_string(),
                name.trim().to_string(),
            ))
        })
        .collect()
}

/// Reorders `data["resources"]` so every resource comes after those it `depends_on`.
///
/// Stable: among resources free to go next, the one written first goes first, so an overlay that declares
/// nothing keeps its order, and one that declares a little moves only what it must. An unknown reference
/// or a cycle is an error naming the resources involved.
pub fn order_by_dependencies(data: &mut YamlValue) -> Result<()> {
    let Some(resources) = data["resources"].as_sequence() else {
        return Ok(());
    };
    if resources.iter().all(|res| res["depends_on"].is_null()) {
        return Ok(());
    }

    let refs = resource_refs(data)?;
    let mut after: Vec<Vec<usize>> = vec![Vec::new(); refs.len()];

    for (i, res) in resources.iter().enumerate() {
        for (text, kind, name) in dependency_entries(res, &refs[i])? {
            let targets: Vec<usize> = refs
                .iter()
                .enumerate()
                // `kinds_match` tolerates a plural on its second argument, so the written kind goes there.
                .filter(|(_, r)| kinds_match(&r.kind, &kind) && r.name.eq_ignore_ascii_case(&name))
                .map(|(j, _)| j)
                .collect();
            ensure!(
                !targets.is_empty(),
                "{} depends_on `{text}`, which the overlay does not declare",
                refs[i]
            );
            ensure!(
                !targets.contains(&i),
                "{} depends_on itself (`{text}`)",
                refs[i]
            );
            after[i].extend(targets);
        }
    }

    let mut placed = vec![false; refs.len()];
    let mut order = Vec::with_capacity(refs.len());
    while order.len() < refs.len() {
        let next = (0..refs.len()).find(|&i| !placed[i] && after[i].iter().all(|&d| placed[d]));
        let Some(i) = next else {
            let stuck: Vec<String> = (0..refs.len())
                .filter(|&i| !placed[i])
                .map(|i| {
                    let waiting: Vec<String> = after[i]
                        .iter()
                        .filter(|&&d| !placed[d])
                        .map(|&d| refs[d].to_string())
                        .collect();
                    format!("  {} waits for {}", refs[i], waiting.join(", "))
                })
                .collect();
            bail!(
                "depends_on forms a cycle, so there is no order to apply these in:\n{}",
                stuck.join("\n")
            );
        };
        placed[i] = true;
        order.push(i);
    }

    let reordered: Vec<YamlValue> = order.iter().map(|&i| resources[i].clone()).collect();
    data["resources"] = YamlValue::Sequence(reordered);
    Ok(())
}

/// What a resource's `wait` asks for, checked against its kind.
pub fn gate_of(res: &YamlValue, who: &ResourceRef) -> Result<Option<(WaitFor, Option<u64>)>> {
    let wait = &res["wait"];
    if wait.is_null() {
        ensure!(
            res["wait_timeout"].is_null(),
            "{who}: wait_timeout needs a `wait:` to apply to"
        );
        return Ok(None);
    }

    let word = wait
        .as_str()
        .with_context(|| format!("{who}: wait must be `complete` or `ready`"))?;
    let wait_for = match word {
        "complete" => {
            ensure!(
                kinds_match("job", &who.kind),
                "{who}: `wait: complete` is for a job; a {} can be `wait: ready`",
                who.kind
            );
            WaitFor::Complete
        }
        "ready" => {
            ensure!(
                ["deployment", "statefulset", "daemonset"]
                    .iter()
                    .any(|k| kinds_match(k, &who.kind)),
                "{who}: `wait: ready` is for a deployment, statefulset or daemonset; a job can be `wait: complete`"
            );
            WaitFor::Ready
        }
        other => bail!("{who}: wait must be `complete` or `ready`, not `{other}`"),
    };

    let timeout = match &res["wait_timeout"] {
        YamlValue::Null => None,
        value => Some(
            value
                .as_u64()
                .filter(|seconds| *seconds > 0)
                .with_context(|| {
                    format!("{who}: wait_timeout must be a positive number of seconds")
                })?,
        ),
    };
    Ok(Some((wait_for, timeout)))
}

/// Every gate an overlay declares, as `(resource, wait, timeout)`, in the overlay's order.
pub fn gates_of(data: &YamlValue) -> Result<Vec<(ResourceRef, WaitFor, Option<u64>)>> {
    let refs = resource_refs(data)?;
    let Some(resources) = data["resources"].as_sequence() else {
        return Ok(Vec::new());
    };
    let mut gates = Vec::new();
    for (res, who) in resources.iter().zip(refs) {
        if let Some((wait, timeout)) = gate_of(res, &who)? {
            gates.push((who, wait, timeout));
        }
    }
    Ok(gates)
}
