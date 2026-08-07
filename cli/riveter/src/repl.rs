use crate::env::{current_env, env_list, env_set, env_show};
use crate::help;
use crate::render::{
    RenderedManifest, ResourceRef, ResourceScope, Selector, generate_manifests_selected,
    list_resources,
};
use quench_cli::prelude::{Tone, print_box_banner, print_status, repl_prompt, require_binary};
use std::process::Command;

pub fn ok(msg: &str) {
    print_status(Tone::Success, "ok", msg);
}

pub fn warn(msg: &str) {
    print_status(Tone::Warn, "warn", msg);
}

fn prompt() -> String {
    let env = current_env().unwrap_or_else(|_| "unset".into());
    repl_prompt("riveter", &env)
}

/// Prints without panicking when stdout is a closed pipe (`help | head`).
fn print_block(text: &str) {
    use std::io::Write;

    let stdout = std::io::stdout();
    let _ = writeln!(stdout.lock(), "{text}");
}

fn repl_help(topic: Option<&str>) {
    let Some(topic) = topic else {
        print_block(&help::overview());
        return;
    };

    if topic == "targets" || topic == "target" {
        print_block(&help::targets());
    } else if let Some(cmd) = help::find_on(topic, help::Surface::Repl) {
        print_block(&help::detail(cmd));
    } else {
        warn(&help::unknown_topic(topic, help::Surface::Repl));
    }
}

fn handle_repl_command(input: &str) -> anyhow::Result<bool> {
    let args = input.split_whitespace().collect::<Vec<_>>();
    if args.is_empty() {
        return Ok(false);
    }

    match args[0] {
        "help" | "h" => {
            repl_help(args.get(1).copied());
        }

        "exit" | "quit" | "q" => {
            return Ok(true);
        }

        "env" if args.len() > 1 && args[1] == "list" => {
            env_list()?;
        }

        "env" if args.len() > 2 && args[1] == "set" => {
            env_set(args[2])?;
            ok(&format!("environment set to {}", args[2]));
        }

        "env" if args.len() > 1 && args[1] == "show" => {
            env_show()?;
        }

        "list" | "ls" => {
            let env = current_env()?;
            let parsed = parse_args(&args, ResourceScope::All, false)?;

            let resources: Vec<ResourceRef> = list_resources(&env)?
                .into_iter()
                .filter(|r| parsed.selector.matches(&r.kind, &r.name) && r.in_scope(parsed.scope))
                .collect();
            print_resource_list(&resources);
        }

        "render" | "r" => {
            let env = current_env()?;
            let parsed = parse_args(&args, ResourceScope::Mutable, false)?;
            let rendered = generate_manifests_selected(&env, parsed.scope, &parsed.selector)?;
            ok(&format!(
                "rendered {} resource(s) to {}",
                rendered.resource_count, rendered.path
            ));
            if let Some(note) = note_skipped(&rendered) {
                warn(&note);
            }
        }

        "diff" | "df" => {
            let env = current_env()?;
            let parsed = parse_args(&args, ResourceScope::Mutable, false)?;

            let (rendered, differs) = kubectl_diff(&env, parsed.scope, &parsed.selector)?;
            if rendered.resource_count == 0 {
                ok("no resources matched selected scope");
            } else if differs {
                warn("the cluster differs from these manifests");
            } else {
                ok("cluster matches these manifests");
            }
        }

        "prune" => {
            let env = current_env()?;
            let dry = args.contains(&"--dry-run");
            report_prune(&prune(&env, dry)?, dry);
        }

        "apply" | "a" => {
            let env = current_env()?;
            let parsed = parse_args(&args, ResourceScope::Mutable, true)?;
            let dry = parsed.dry_run;
            let wait = WaitPolicy {
                enabled: !args.contains(&"--no-wait"),
                ..WaitPolicy::default()
            };

            let rendered = kubectl_apply(&env, dry, parsed.scope, &parsed.selector, wait)?;
            if rendered.resource_count == 0 {
                ok("no resources matched selected scope");
            } else {
                let verb = if dry { "would apply" } else { "applied" };
                ok(&format!(
                    "{verb} {} resource(s): {}",
                    rendered.resource_count,
                    describe(&rendered)
                ));
            }
        }

        "delete" | "del" | "d" => {
            let env = current_env()?;
            let parsed = parse_args(&args, ResourceScope::Mutable, false)?;

            let rendered = kubectl_delete(&env, parsed.scope, &parsed.selector)?;
            if rendered.resource_count == 0 {
                ok("no resources matched selected scope");
            } else {
                warn(&format!(
                    "deleted {} resource(s) for env {env}: {}",
                    rendered.resource_count,
                    describe(&rendered)
                ));
            }
        }

        "images" => repl_images(&args)?,

        _ => {
            warn("unknown command — type `help`");
        }
    }

    Ok(false)
}

fn repl_images(args: &[&str]) -> anyhow::Result<()> {
    let update = args.contains(&"--update");
    let registry_auth = args
        .windows(2)
        .filter(|pair| pair[0] == "--registry-auth")
        .map(|pair| pair[1].to_string())
        .collect::<Vec<_>>();
    crate::image_updates::check_image_updates(
        std::path::Path::new(crate::env::OVERLAY_DIR),
        update,
        &registry_auth,
    )
}

fn error(msg: &str) {
    print_status(Tone::Error, "error", msg);
}

pub fn repl() -> anyhow::Result<()> {
    use rustyline::{DefaultEditor, error::ReadlineError};

    print_box_banner("Riveter REPL", "env-aware manifest commands");
    print_status(Tone::Info, "hint", "type `help` to list commands");

    let mut rl = DefaultEditor::new()?;

    loop {
        let prompt = prompt();
        match rl.readline(&prompt) {
            Ok(line) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                rl.add_history_entry(line)?;
                match handle_repl_command(line) {
                    Ok(exit) => {
                        if exit {
                            break;
                        }
                    }
                    Err(e) => {
                        error(&format!("{e:#}"));
                    }
                }
            }
            Err(ReadlineError::Interrupted | ReadlineError::Eof) => break,
            Err(e) => return Err(e.into()),
        }
    }

    Ok(())
}

/// A kubectl invocation bound to the overlay's context when it pins one, so the
/// environment decides the cluster rather than the shell's ambient state.
fn kubectl(rendered: &RenderedManifest) -> Command {
    let mut cmd = Command::new("kubectl");
    if let Some(context) = &rendered.kube_context {
        cmd.args(["--context", context]);
    }
    cmd
}

/// The context kubectl would pick on its own.
fn current_kube_context() -> Option<String> {
    let out = Command::new("kubectl")
        .args(["config", "current-context"])
        .output()
        .ok()?;

    if !out.status.success() {
        return None;
    }

    let context = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!context.is_empty()).then_some(context)
}

/// Says which cluster is about to be touched.
///
/// An overlay that sets `kube_context` gets that binding enforced. One that does
/// not is at the mercy of whatever `kubectl config current-context` happens to
/// be — for a tool whose job is multi-environment deploys, that is worth saying
/// out loud rather than discovering afterwards.
fn announce_target(env: &str, rendered: &RenderedManifest) {
    if let Some(context) = &rendered.kube_context {
        print_status(Tone::Info, "context", &format!("{env} -> {context}"));
        return;
    }

    let current = current_kube_context().unwrap_or_else(|| "unknown".to_string());
    warn(&format!(
        "{env} pins no kube_context, so this uses kubectl's current context `{current}` — \
         add `kube_context: <name>` to overlays/{env}/overlay.yaml to bind the environment \
         to its cluster"
    ));
}

/// Whether the target namespace is known to be absent.
///
/// `--ignore-not-found` turns "absent" into a successful, empty result, so a
/// cluster we simply cannot reach stays distinguishable from one where the
/// namespace really is missing — only the latter is worth failing on.
fn namespace_is_absent(rendered: &RenderedManifest, namespace: &str) -> Option<bool> {
    let out = kubectl(rendered)
        .args([
            "get",
            "namespace",
            namespace,
            "--ignore-not-found",
            "-o",
            "name",
        ])
        .output()
        .ok()?;

    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().is_empty())
}

/// Fails before kubectl does when the environment's namespace does not exist
/// and this apply would not create it.
///
/// Marking `namespace` immutable is the documented pattern, but the default
/// scope then excludes it, so bootstrapping a fresh environment otherwise dies
/// one confusing `namespaces "x" not found` at a time — once per resource.
fn ensure_namespace_exists(env: &str, rendered: &RenderedManifest) -> anyhow::Result<()> {
    if rendered.creates_namespace {
        return Ok(());
    }

    let Some(namespace) = &rendered.namespace else {
        return Ok(());
    };

    if namespace_is_absent(rendered, namespace) != Some(true) {
        return Ok(());
    }

    let declares_namespace = rendered
        .skipped_out_of_scope
        .iter()
        .any(|r| r.kind.eq_ignore_ascii_case("namespace"));

    anyhow::ensure!(
        !declares_namespace,
        "namespace `{namespace}` does not exist, and this scope excludes the \
         `namespace` resource {env} declares\n\n\
         run `riveter apply --scope all` to create it first"
    );

    anyhow::bail!(
        "namespace `{namespace}` does not exist, and {env} declares no `namespace` \
         resource to create it\n\n\
         add one to overlays/{env}/overlay.yaml, or create the namespace yourself"
    );
}

/// How long an apply waits for each rollout, and whether it waits at all.
#[derive(Debug, Clone, Copy)]
pub struct WaitPolicy {
    pub enabled: bool,
    pub timeout_seconds: u64,
}

impl Default for WaitPolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            timeout_seconds: 300,
        }
    }
}

/// Kinds whose readiness `kubectl rollout status` can report on.
const ROLLOUT_KINDS: &[&str] = &["deployment", "statefulset", "daemonset"];

/// Waits for everything just applied to actually become ready.
///
/// kubectl accepting a manifest only means the API server stored it. Without
/// this, a rollout that never starts a healthy pod still reports success, which
/// is precisely the case where being told the truth matters.
fn await_rollouts(rendered: &RenderedManifest, wait: WaitPolicy) -> anyhow::Result<()> {
    let rollouts: Vec<&ResourceRef> = rendered
        .selected
        .iter()
        .filter(|r| {
            ROLLOUT_KINDS
                .iter()
                .any(|k| k.eq_ignore_ascii_case(&r.kind))
        })
        .collect();

    if rollouts.is_empty() {
        return Ok(());
    }

    for res in rollouts {
        print_status(
            Tone::Info,
            "wait",
            &format!("{res} (up to {}s)", wait.timeout_seconds),
        );

        let mut cmd = kubectl(rendered);
        cmd.args(["rollout", "status"]);
        if let Some(namespace) = &rendered.namespace {
            cmd.args(["-n", namespace]);
        }
        let status = cmd
            .arg(format!("{}/{}", res.kind.to_lowercase(), res.name))
            .arg(format!("--timeout={}s", wait.timeout_seconds))
            .status()?;

        anyhow::ensure!(
            status.success(),
            "{res} did not become ready within {}s — the manifests were applied, \
             but the rollout has not completed\n\n\
             inspect it with `kubectl rollout status {}/{}`, or pass `--no-wait` \
             to skip this check",
            wait.timeout_seconds,
            res.kind.to_lowercase(),
            res.name
        );
    }

    Ok(())
}

pub fn kubectl_apply(
    env: &str,
    dry: bool,
    scope: ResourceScope,
    selector: &Selector,
    wait: WaitPolicy,
) -> anyhow::Result<RenderedManifest> {
    require_binary("kubectl", "riveter shells out to it to touch the cluster")?;
    let rendered = generate_manifests_selected(env, scope, selector)?;
    if rendered.resource_count == 0 {
        return Ok(rendered);
    }

    announce_target(env, &rendered);

    // A client-side dry run never reaches the cluster, so there is nothing to
    // check against and nothing that could fail on a missing namespace.
    if !dry {
        ensure_namespace_exists(env, &rendered)?;
    }

    let mut cmd = kubectl(&rendered);
    cmd.arg("apply");
    if dry {
        cmd.arg("--dry-run=client");
    }
    let status = cmd.arg("-f").arg(&rendered.path).status()?;
    anyhow::ensure!(status.success(), "kubectl apply failed");

    if !dry && wait.enabled {
        await_rollouts(&rendered, wait)?;
    }

    Ok(rendered)
}

/// Shows what applying would change, via `kubectl diff`.
///
/// Returns whether anything differs. `kubectl diff` exits 1 to mean "there is a
/// diff", which is not an error, so only a higher code is treated as one.
pub fn kubectl_diff(
    env: &str,
    scope: ResourceScope,
    selector: &Selector,
) -> anyhow::Result<(RenderedManifest, bool)> {
    require_binary("kubectl", "riveter shells out to it to touch the cluster")?;
    let rendered = generate_manifests_selected(env, scope, selector)?;
    if rendered.resource_count == 0 {
        return Ok((rendered, false));
    }

    announce_target(env, &rendered);

    let status = kubectl(&rendered)
        .args(["diff", "-f", &rendered.path])
        .status()?;

    match status.code() {
        Some(0) => Ok((rendered, false)),
        Some(1) => Ok((rendered, true)),
        _ => anyhow::bail!("kubectl diff failed"),
    }
}

pub fn kubectl_delete(
    env: &str,
    scope: ResourceScope,
    selector: &Selector,
) -> anyhow::Result<RenderedManifest> {
    require_binary("kubectl", "riveter shells out to it to touch the cluster")?;
    let rendered = generate_manifests_selected(env, scope, selector)?;
    if rendered.resource_count == 0 {
        return Ok(rendered);
    }

    announce_target(env, &rendered);

    let status = kubectl(&rendered)
        .args(["delete", "-f", &rendered.path])
        .status()?;
    anyhow::ensure!(status.success(), "kubectl delete failed");
    Ok(rendered)
}

/// The label every riveter template stamps on what it creates. Together with
/// `env`, it identifies the resources one environment owns.
const MANAGED_BY: &str = "app.kubernetes.io/managed-by=riveter";

/// One live resource riveter owns, as `kind/name` from `kubectl -o name`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct LiveResource {
    pub kind: String,
    pub name: String,
}

impl std::fmt::Display for LiveResource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.kind, self.name)
    }
}

/// Asks the cluster what it holds for this environment.
///
/// Queries the kinds riveter can render, since a resource it never created is
/// not its to remove. `raw` resources are unreachable this way — they carry
/// whatever labels the overlay wrote — so they are never pruned.
fn live_resources(env: &str, rendered: &RenderedManifest) -> anyhow::Result<Vec<LiveResource>> {
    let kinds = crate::render::prunable_kinds().join(",");
    let selector = format!("{MANAGED_BY},env={env}");

    let mut cmd = kubectl(rendered);
    cmd.args(["get", &kinds, "-l", &selector, "-o", "name"]);
    if let Some(namespace) = &rendered.namespace {
        cmd.args(["-n", namespace]);
    }
    // Kinds the cluster does not serve (a CRD that is not installed) would
    // otherwise abort the whole query.
    cmd.arg("--ignore-not-found");

    let out = cmd.output()?;
    anyhow::ensure!(
        out.status.success(),
        "could not list live resources for {env}:\n{}",
        String::from_utf8_lossy(&out.stderr).trim()
    );

    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| {
            let (kind, name) = line.trim().split_once('/')?;
            // `kubectl -o name` prints the plural, group-qualified form.
            let kind = kind.split('.').next().unwrap_or(kind);
            Some(LiveResource {
                kind: kind.to_string(),
                name: name.to_string(),
            })
        })
        .collect())
}

/// Live resources this environment owns that the overlay no longer declares.
pub fn find_orphans(env: &str, rendered: &RenderedManifest) -> anyhow::Result<Vec<LiveResource>> {
    let live = live_resources(env, rendered)?;

    let mut orphans: Vec<LiveResource> = live
        .into_iter()
        .filter(|l| {
            !rendered.selected.iter().any(|r| {
                r.name.eq_ignore_ascii_case(&l.name) && crate::render::kinds_match(&r.kind, &l.kind)
            })
        })
        .collect();

    orphans.sort();
    Ok(orphans)
}

/// Removes what the overlay stopped declaring.
///
/// Without this an entry deleted from an overlay lives on in the cluster
/// forever: `delete` only ever removes what the overlay still renders, so the
/// resource becomes invisible to every riveter command.
pub fn prune(env: &str, dry_run: bool) -> anyhow::Result<Vec<LiveResource>> {
    require_binary("kubectl", "riveter shells out to it to touch the cluster")?;
    // The full overlay, so a resource excluded only by scope is not mistaken
    // for one the overlay has dropped.
    let rendered = generate_manifests_selected(env, ResourceScope::All, &Selector::default())?;
    announce_target(env, &rendered);

    let orphans = find_orphans(env, &rendered)?;
    if orphans.is_empty() || dry_run {
        return Ok(orphans);
    }

    for orphan in &orphans {
        let mut cmd = kubectl(&rendered);
        cmd.args(["delete", &format!("{}/{}", orphan.kind, orphan.name)]);
        if let Some(namespace) = &rendered.namespace {
            cmd.args(["-n", namespace]);
        }

        let status = cmd.status()?;
        anyhow::ensure!(status.success(), "failed to delete {orphan}");
    }

    Ok(orphans)
}

/// Reports what `prune` found, or would remove.
pub fn report_prune(orphans: &[LiveResource], dry_run: bool) {
    if orphans.is_empty() {
        ok("nothing to prune — the cluster matches the overlay");
        return;
    }

    let list = orphans
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ");

    if dry_run {
        warn(&format!(
            "{} resource(s) the overlay no longer declares: {list}",
            orphans.len()
        ));
    } else {
        warn(&format!("pruned {} resource(s): {list}", orphans.len()));
    }
}

/// Names what the scope left behind, so resources missing from a render are
/// visible rather than silently absent.
#[must_use]
pub fn note_skipped(rendered: &RenderedManifest) -> Option<String> {
    let skipped = &rendered.skipped_out_of_scope;
    if skipped.is_empty() {
        return None;
    }

    Some(format!(
        "{} resource(s) outside this scope: {} — `--scope all` includes them",
        skipped.len(),
        skipped
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

/// `kind/name, kind/name` for reporting what a command touched.
pub fn describe(rendered: &RenderedManifest) -> String {
    rendered
        .selected
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn print_resource_list(resources: &[ResourceRef]) {
    use std::io::Write;

    if resources.is_empty() {
        warn("no resources matched");
        return;
    }

    let kind_width = resources.iter().map(|r| r.kind.len()).max().unwrap_or(4);
    let name_width = resources.iter().map(|r| r.name.len()).max().unwrap_or(4);

    // Written directly so a closed pipe (`riveter list | head`) ends the loop
    // instead of panicking the way `println!` does.
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    for res in resources {
        let lifecycle = if res.immutable {
            "immutable"
        } else {
            "mutable"
        };
        if writeln!(
            out,
            "  {:<kind_width$}  {:<name_width$}  {lifecycle}",
            res.kind, res.name
        )
        .is_err()
        {
            break;
        }
    }
}

/// The flags and targets of one REPL command line.
#[derive(Debug)]
#[doc(hidden)]
pub struct ParsedArgs {
    pub scope: ResourceScope,
    pub dry_run: bool,
    pub selector: Selector,
}

/// Parses a REPL command's arguments in one pass.
///
/// Anything that is not a recognised flag, a recognised flag's value, or a
/// resource target is an error. Silently dropping unknown tokens would let
/// `apply --dry-runn` reach the cluster for real, so the parser fails closed.
#[doc(hidden)]
pub fn parse_args(
    args: &[&str],
    default: ResourceScope,
    allow_dry_run: bool,
) -> anyhow::Result<ParsedArgs> {
    let mut scope = default;
    let mut dry_run = false;
    let mut targets = Vec::new();
    let mut idx = 1;

    while idx < args.len() {
        let arg = args[idx];
        idx += 1;

        if let Some(rest) = arg.strip_prefix("--scope") {
            // `--scope value` and `--scope=value` both work on the CLI, where
            // clap accepts either, so the REPL has to take both as well.
            let raw = if let Some(inline) = rest.strip_prefix('=') {
                inline
            } else if rest.is_empty() {
                let next = args.get(idx).copied();
                idx += 1;
                next.unwrap_or("")
            } else {
                anyhow::bail!("unknown option `{arg}` (did you mean `--scope`?)");
            };

            anyhow::ensure!(
                !raw.is_empty(),
                "missing value for --scope (expected mutable|immutable|all)"
            );
            scope = parse_scope_value(raw)?;
        } else if arg == "--dry-run" && allow_dry_run {
            dry_run = true;
        } else if arg.starts_with('-') {
            anyhow::bail!("{}", unknown_option(arg, allow_dry_run));
        } else {
            targets.push(arg.to_string());
        }
    }

    Ok(ParsedArgs {
        scope,
        dry_run,
        selector: Selector::parse(&targets)?,
    })
}

fn parse_scope_value(raw: &str) -> anyhow::Result<ResourceScope> {
    match raw.to_ascii_lowercase().as_str() {
        "mutable" => Ok(ResourceScope::Mutable),
        "immutable" => Ok(ResourceScope::Immutable),
        "all" => Ok(ResourceScope::All),
        _ => anyhow::bail!("invalid --scope value `{raw}` (expected mutable|immutable|all)"),
    }
}

/// Names what the command actually accepts, so the error carries the fix.
fn unknown_option(arg: &str, allow_dry_run: bool) -> String {
    let hint = if arg == "--dry-run" {
        " (only `apply` takes --dry-run)"
    } else {
        ""
    };
    let accepted = if allow_dry_run {
        "--scope <mutable|immutable|all>, --dry-run"
    } else {
        "--scope <mutable|immutable|all>"
    };

    format!("unknown option `{arg}`{hint}\n\naccepted options: {accepted}")
}
