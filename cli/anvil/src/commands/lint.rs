use quench_core::{Context, Result};
use quench_serde::Value;
use std::collections::{BTreeMap, HashSet};
use std::fmt::Write as _;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};

use crate::util::{OutputMode, output_mode, run_command, run_command_json};

#[must_use]
pub fn lint_args(
    all_targets: bool,
    all_features: bool,
    deny_warnings: bool,
    json: bool,
) -> Vec<String> {
    let mut args = vec!["clippy".to_string()];

    // Clippy's own flag - a per-line stream of rustc/clippy diagnostic JSON,
    // already stable and used by tools like rust-analyzer.
    if json {
        args.push("--message-format=json".to_string());
    }

    if all_targets {
        args.push("--all-targets".to_string());
    }

    if all_features {
        args.push("--all-features".to_string());
    }

    args.push("--".to_string());

    if deny_warnings {
        args.push("-D".to_string());
        args.push("warnings".to_string());
    }

    args
}

/// Extracts the package name from a cargo `package_id`, in either the
/// `path+file:///dir/name#name@1.0.0` / `...#1.0.0` form or the older
/// `name 1.0.0 (path+file:///dir)` form.
#[must_use]
pub fn package_name(package_id: &str) -> String {
    if let Some((url, frag)) = package_id.split_once('#') {
        return match frag.split_once('@') {
            Some((name, _)) => name.to_string(),
            // Bare version fragment: the name is the last path component.
            None => url.rsplit('/').next().unwrap_or(url).to_string(),
        };
    }
    package_id
        .split_whitespace()
        .next()
        .unwrap_or(package_id)
        .to_string()
}

/// Folds one line of cargo's JSON message stream into `counts`, returning the
/// rendered diagnostic when the line is a real (non-summary) warning/error.
///
/// Identical diagnostics (the same lint hit again by a lib and its test
/// target) are counted once per package, tracked through `seen`.
pub fn tally_diagnostic<S: std::hash::BuildHasher>(
    line: &str,
    counts: &mut BTreeMap<String, usize>,
    seen: &mut HashSet<(String, String), S>,
) -> Option<String> {
    let v: Value = quench_serde::json::from_str(line).ok()?;
    if v["reason"] != "compiler-message" {
        return None;
    }
    let msg = &v["message"];
    if !matches!(msg["level"].as_str(), Some("warning" | "error")) {
        return None;
    }
    // "aborting due to N previous errors" / "N warnings emitted" carry
    // neither a code nor a span.
    let has_span = msg["spans"].as_array().is_some_and(|s| !s.is_empty());
    if !has_span && msg["code"].is_null() {
        return None;
    }
    let rendered = msg["rendered"].as_str()?.to_string();
    let package = package_name(v["package_id"].as_str().unwrap_or("unknown"));
    if seen.insert((package.clone(), rendered.clone())) {
        *counts.entry(package).or_default() += 1;
    }
    Some(rendered)
}

/// Renders the per-module summary, most issues first.
#[must_use]
pub fn format_summary(counts: &BTreeMap<String, usize>) -> String {
    let mut rows: Vec<(&String, &usize)> = counts.iter().collect();
    rows.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    let width = rows
        .iter()
        .map(|(m, _)| m.len())
        .max()
        .unwrap_or(0)
        .max("module".len());
    let mut out = String::new();
    let _ = writeln!(out, "{:<width$}  issues", "module");
    for (module, n) in &rows {
        let _ = writeln!(out, "{module:<width$}  {n}");
    }
    let total: usize = counts.values().sum();
    let _ = writeln!(out, "{:<width$}  {total}", "total");
    out
}

pub fn lint(all_targets: bool, all_features: bool, deny_warnings: bool, json: bool) -> Result<()> {
    let mut cmd = Command::new("cargo");
    cmd.args(lint_args(all_targets, all_features, deny_warnings, json));

    if json {
        return run_command_json(cmd);
    }

    // Clippy runs with JSON messages so diagnostics can be tallied per
    // module; the rendered (colored) text is replayed on stderr as it
    // arrives, so the live output matches a plain `cargo clippy`.
    let mut args = lint_args(all_targets, all_features, deny_warnings, false);
    args.insert(
        1,
        "--message-format=json-diagnostic-rendered-ansi".to_string(),
    );
    let mut cmd = Command::new("cargo");
    cmd.args(args).stdout(Stdio::piped());

    let mut child = cmd.spawn().context("Failed to execute cargo clippy")?;
    let stdout = child.stdout.take().context("no stdout from cargo clippy")?;
    let stream_live = matches!(output_mode(), OutputMode::Full);

    let mut counts = BTreeMap::new();
    let mut seen = HashSet::new();
    for line in BufReader::new(stdout).lines() {
        let line = line.context("Failed to read cargo clippy output")?;
        if let Some(rendered) = tally_diagnostic(&line, &mut counts, &mut seen)
            && stream_live
        {
            eprint!("{rendered}");
        }
    }
    let status = child.wait().context("Failed to wait for cargo clippy")?;

    if !counts.is_empty() {
        eprintln!("\nLint issues per module:\n{}", format_summary(&counts));
    }
    if !status.success() {
        quench_core::bail!("lint failed with status: {status}");
    }
    Ok(())
}

#[must_use]
pub fn format_args(check: bool) -> Vec<String> {
    // Workspaces may rely on unstable rustfmt options (e.g. `group_imports`,
    // `imports_granularity`) in their `rustfmt.toml`, which stable rustfmt
    // silently ignores rather than applying - so it must run under nightly
    // for `--check` to agree with how the code was actually formatted.
    let mut args = vec!["+nightly".to_string(), "fmt".to_string()];

    if check {
        args.push("--check".to_string());
    }

    args
}

pub fn format(check: bool) -> Result<()> {
    let mut cmd = Command::new("cargo");
    cmd.args(format_args(check));

    run_command(cmd, "format")
}
