//! `riveter diff --against <git-ref>`: what changed in an overlay since a commit, with no cluster.
//!
//! The overlay as it was at the ref is read out of git into a scratch directory and rendered; the
//! overlay as it is now is rendered beside it; and the two manifests are diffed. Both sides go
//! through the same renderer, so the diff is of what riveter would send, not of the overlay's
//! source text - a template change that renders identically is correctly reported as no change.

use crate::env::{Workspace, overlay_dir, with_workspace};
use crate::render::{ResourceScope, Selector, generate_manifests_selected};
use quench_cli::prelude::require_binary;
use quench_core::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// What the comparison found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comparison {
    /// The ref as given, for reporting.
    pub git_ref: String,
    /// Resources the overlay declares now (after scope and targets).
    pub resource_count: usize,
    pub differs: bool,
}

fn git(args: &[&str]) -> Command {
    let mut cmd = Command::new("git");
    cmd.args(args);
    cmd
}

fn git_text(args: &[&str]) -> Result<String> {
    let out = git(args)
        .output()
        .with_context(|| format!("failed to run git {}", args.join(" ")))?;
    quench_core::ensure!(
        out.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&out.stderr).trim()
    );
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// A ref made safe to use as a directory name, so the diff header names it.
fn label_for(git_ref: &str) -> String {
    let label: String = git_ref
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if label == "working-tree" {
        "working-tree-ref".to_string()
    } else {
        label
    }
}

/// Writes the overlays directory as it was at `git_ref` into `dest`.
///
/// Reads each file with `git show` rather than unpacking an archive, so it needs nothing beyond
/// git. `overlays` is relative to the current directory, which may be below the repository root.
fn export_overlays(git_ref: &str, dest: &Path) -> Result<()> {
    let prefix = git_text(&["rev-parse", "--show-prefix"])?;
    let overlays = overlay_dir();
    // A pathspec is relative to the current directory, while `--full-name` makes the names it prints
    // relative to the repository root - hence the prefix is stripped from the results below.
    let pathspec = overlays.display().to_string();

    let listing = git_text(&[
        "ls-tree",
        "-r",
        "--name-only",
        "--full-name",
        "-z",
        git_ref,
        "--",
        &pathspec,
    ])?;
    // `-z` separates with NUL, which `trim` above leaves alone.
    let files: Vec<&str> = listing.split('\0').filter(|f| !f.is_empty()).collect();
    quench_core::ensure!(
        !files.is_empty(),
        "`{}` does not exist at {git_ref}",
        overlays.display()
    );

    for file in files {
        let inside = file
            .strip_prefix(&prefix)
            .and_then(|f| f.strip_prefix(&format!("{}/", overlays.display())))
            .with_context(|| format!("unexpected path from git: {file}"))?;
        let target = dest.join(inside);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let out = git(&["show", &format!("{git_ref}:{file}")])
            .output()
            .with_context(|| format!("failed to read {file} at {git_ref}"))?;
        quench_core::ensure!(
            out.status.success(),
            "git show {git_ref}:{file} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
        fs::write(&target, out.stdout)?;
    }
    Ok(())
}

fn render_to(
    env: &str,
    overlays_dir: Option<PathBuf>,
    output_dir: PathBuf,
    scope: ResourceScope,
    selector: &Selector,
) -> Result<(PathBuf, usize)> {
    let workspace = Workspace {
        overlays_dir,
        output_dir: Some(output_dir),
        ..Workspace::default()
    };
    let rendered = with_workspace(workspace, || {
        generate_manifests_selected(env, scope, selector)
    })?;
    Ok((PathBuf::from(rendered.path), rendered.resource_count))
}

/// Renders `env` as of `git_ref` and as it is now, and prints a unified diff of the two.
///
/// Nothing in the working directory is written: both renders go to a scratch directory.
pub fn diff_against(
    env: &str,
    git_ref: &str,
    scope: ResourceScope,
    selector: &Selector,
) -> Result<Comparison> {
    require_binary("git", "`--against` reads the overlay's history from it")?;
    quench_core::ensure!(!git_ref.trim().is_empty(), "--against needs a git ref");

    // `^{commit}` so a tag or branch resolves, and a blob or tree name does not slip through.
    git_text(&[
        "rev-parse",
        "--verify",
        "--quiet",
        &format!("{git_ref}^{{commit}}"),
    ])
    .map_err(|_| {
        quench_core::err!("`{git_ref}` is not a commit, branch or tag in this repository")
    })?;

    let scratch =
        quench_core::tmp::TempDir::new().context("failed to create a scratch directory")?;
    let old_overlays = scratch.path().join("old-overlays");
    export_overlays(git_ref, &old_overlays)?;
    quench_core::ensure!(
        old_overlays.join(env).join("overlay.yaml").is_file(),
        "environment `{env}` has no overlay at {git_ref}, so there is nothing to compare against"
    );

    // `.env` is not committed, so it is not in the ref; the current one stands in for both sides, so
    // a difference is always the overlay's, never a change of secrets between the two renders.
    let current_env = overlay_dir().join(env).join(".env");
    if current_env.is_file() {
        fs::copy(&current_env, old_overlays.join(env).join(".env"))?;
    }

    let (old_manifest, _) = render_to(
        env,
        Some(old_overlays),
        scratch.path().join("old-out"),
        scope,
        selector,
    )
    .with_context(|| format!("failed to render `{env}` as of {git_ref}"))?;
    let (new_manifest, resource_count) =
        render_to(env, None, scratch.path().join("new-out"), scope, selector)
            .context("failed to render the working tree")?;

    // Relative names inside the scratch directory, so the diff header reads
    // `a/<ref>/<env>.yaml` and `b/working-tree/<env>.yaml` rather than a temp path.
    let old_name = format!("{}/{env}.yaml", label_for(git_ref));
    let new_name = format!("working-tree/{env}.yaml");
    for (from, name) in [(&old_manifest, &old_name), (&new_manifest, &new_name)] {
        let to = scratch.path().join(name);
        fs::create_dir_all(to.parent().context("no parent")?)?;
        fs::copy(from, to)?;
    }

    let status = git(&[
        "diff",
        "--no-index",
        "--no-color",
        "--",
        &old_name,
        &new_name,
    ])
    .current_dir(scratch.path())
    .status()
    .context("failed to run git diff")?;

    // `git diff --no-index` exits 1 for "the files differ", which is a result, not an error.
    match status.code() {
        Some(0) => Ok(Comparison {
            git_ref: git_ref.to_string(),
            resource_count,
            differs: false,
        }),
        Some(1) => Ok(Comparison {
            git_ref: git_ref.to_string(),
            resource_count,
            differs: true,
        }),
        _ => quench_core::bail!("git diff failed"),
    }
}
