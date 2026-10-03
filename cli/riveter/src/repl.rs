use crate::env::{current_env, env_list, env_set, env_show};
use crate::help;
use crate::render::{
    Gate, RenderedManifest, ResourceRef, ResourceScope, Selector, WaitFor,
    generate_manifests_selected, list_resources,
};
use quench_cli::prelude::{
    ReplControl, Tone, print_box_banner, print_status, repl_prompt, repl_run, require_binary,
};
use std::process::Command;

pub fn ok(msg: &str) {
    print_status(Tone::Success, "ok", msg);
}

pub fn warn(msg: &str) {
    print_status(Tone::Warn, "warn", msg);
}

#[must_use]
pub fn prompt() -> String {
    let env = current_env().unwrap_or_else(|_| "unset".into());
    repl_prompt("riveter", &env)
}

/// Prints without panicking when stdout is a closed pipe (`help | head`).
pub fn print_block(text: &str) {
    use std::io::Write;

    let stdout = std::io::stdout();
    let _ = writeln!(stdout.lock(), "{text}");
}

pub fn repl_help(topic: Option<&str>) {
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

pub fn handle_repl_command(input: &str) -> anyhow::Result<bool> {
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

        "validate" => repl_validate(&args)?,

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

fn repl_validate(args: &[&str]) -> anyhow::Result<()> {
    let env = current_env()?;
    let parsed = parse_args(args, ResourceScope::All, false)?;
    crate::schema_cmd::validate_env(&env, parsed.scope, &parsed.selector)
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

pub fn error(msg: &str) {
    print_status(Tone::Error, "error", msg);
}

pub fn repl() -> anyhow::Result<()> {
    print_box_banner("Riveter REPL", "env-aware manifest commands");
    print_status(Tone::Info, "hint", "type `help` to list commands");

    repl_run(prompt(), |line| match handle_repl_command(line) {
        Ok(true) => ReplControl::Exit,
        Ok(false) => ReplControl::Continue(prompt()),
        Err(e) => {
            error(&format!("{e:#}"));
            ReplControl::Continue(prompt())
        }
    })?;

    Ok(())
}

/// A kubectl invocation bound to the overlay's context when it pins one, so the
/// environment decides the cluster rather than the shell's ambient state.
#[must_use]
pub fn kubectl(rendered: &RenderedManifest) -> Command {
    let mut cmd = Command::new("kubectl");
    if let Some(context) = &rendered.kube_context {
        cmd.args(["--context", context]);
    }
    cmd
}

/// The context kubectl would pick on its own.
#[must_use]
pub fn current_kube_context() -> Option<String> {
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
pub fn announce_target(env: &str, rendered: &RenderedManifest) {
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
         re-run with `--scope all` to create it first"
    );

    anyhow::bail!(
        "namespace `{namespace}` does not exist, and {env} declares no `namespace` \
         resource to create it\n\n\
         add one to overlays/{env}/overlay.yaml, or create the namespace yourself"
    );
}

/// Creates the environment's namespace if the cluster does not have it, and says whether it did.
///
/// For `install`, which puts a package into a cluster that may never have seen it: the namespace is an
/// immutable resource, so the default scope leaves it out, and the first install would otherwise stop at
/// "re-run with `--scope all`". A cluster that cannot be reached is not mistaken for a missing namespace
/// (see [`namespace_is_absent`]), and an overlay that declares no namespace is left to the apply to
/// report on.
pub fn create_namespace_if_missing(env: &str) -> anyhow::Result<bool> {
    require_binary("kubectl", "riveter shells out to it to touch the cluster")?;

    let declares = crate::render::list_resources(env)?
        .iter()
        .any(|r| r.kind.eq_ignore_ascii_case("namespace"));
    if !declares {
        return Ok(false);
    }

    let rendered =
        generate_manifests_selected(env, ResourceScope::All, &Selector::parse(&["namespace"])?)?;
    let Some(namespace) = rendered.namespace.clone() else {
        return Ok(false);
    };
    if namespace_is_absent(&rendered, &namespace) != Some(true) {
        return Ok(false);
    }

    print_status(Tone::Info, "namespace", &format!("creating {namespace}"));
    let status = kubectl(&rendered)
        .args(["apply", "-f", &rendered.path])
        .status()?;
    anyhow::ensure!(status.success(), "could not create namespace {namespace}");
    Ok(true)
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
        await_rollout(rendered, res, wait.timeout_seconds)?;
    }

    Ok(())
}

/// Waits for one `Deployment`, `StatefulSet` or `DaemonSet` to finish rolling out.
fn await_rollout(
    rendered: &RenderedManifest,
    res: &ResourceRef,
    timeout_seconds: u64,
) -> anyhow::Result<()> {
    print_status(
        Tone::Info,
        "wait",
        &format!("{res} (up to {timeout_seconds}s)"),
    );

    let mut cmd = kubectl(rendered);
    cmd.args(["rollout", "status"]);
    if let Some(namespace) = &rendered.namespace {
        cmd.args(["-n", namespace]);
    }
    let status = cmd
        .arg(format!("{}/{}", res.kind.to_lowercase(), res.name))
        .arg(format!("--timeout={timeout_seconds}s"))
        .status()?;

    anyhow::ensure!(
        status.success(),
        "{res} did not become ready within {timeout_seconds}s — the manifests were applied, \
         but the rollout has not completed\n\n\
         inspect it with `kubectl rollout status {}/{}`, or pass `--no-wait` \
         to skip this check",
        res.kind.to_lowercase(),
        res.name
    );

    Ok(())
}

/// What `kubectl get job` is asked to print: whether it has completed, whether it has failed, and why.
const JOB_STATE: &str = r#"jsonpath={.status.conditions[?(@.type=="Complete")].status}|{.status.conditions[?(@.type=="Failed")].status}|{.status.conditions[?(@.type=="Failed")].message}"#;

/// Where a Job is, as far as its conditions say.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobState {
    /// Neither completed nor failed yet.
    Running,
    /// Finished successfully.
    Complete,
    /// Gave up, with the reason Kubernetes recorded.
    Failed(String),
}

/// Reads [`JOB_STATE`] output: `<Complete>|<Failed>|<message>`.
#[must_use]
pub fn parse_job_state(output: &str) -> JobState {
    let mut fields = output.trim_end().splitn(3, '|');
    let complete = fields.next().unwrap_or_default().trim();
    let failed = fields.next().unwrap_or_default().trim();
    let message = fields.next().unwrap_or_default().trim();

    if complete.eq_ignore_ascii_case("true") {
        JobState::Complete
    } else if failed.eq_ignore_ascii_case("true") {
        JobState::Failed(message.to_string())
    } else {
        JobState::Running
    }
}

/// Waits for a Job to complete, and stops the moment it has failed rather than at the timeout.
///
/// `kubectl wait --for=condition=complete` would sit out the whole timeout on a Job that has already
/// exhausted its retries, which for a migration is exactly the case worth being told about at once.
fn await_job(
    rendered: &RenderedManifest,
    res: &ResourceRef,
    timeout_seconds: u64,
) -> anyhow::Result<()> {
    print_status(
        Tone::Info,
        "wait",
        &format!("{res} to complete (up to {timeout_seconds}s)"),
    );

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_seconds);
    let interval = std::time::Duration::from_millis((timeout_seconds * 250).clamp(100, 2000));

    loop {
        let mut cmd = kubectl(rendered);
        cmd.args(["get", "job", &res.name]);
        if let Some(namespace) = &rendered.namespace {
            cmd.args(["-n", namespace]);
        }
        let out = cmd.args(["-o", JOB_STATE]).output()?;
        anyhow::ensure!(
            out.status.success(),
            "could not read {res}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );

        match parse_job_state(&String::from_utf8_lossy(&out.stdout)) {
            JobState::Complete => return Ok(()),
            JobState::Failed(reason) => anyhow::bail!(
                "{res} failed{}\n\ninspect it with `kubectl logs job/{}`",
                if reason.is_empty() {
                    String::new()
                } else {
                    format!(": {reason}")
                },
                res.name
            ),
            JobState::Running => {}
        }

        anyhow::ensure!(
            std::time::Instant::now() < deadline,
            "{res} did not complete within {timeout_seconds}s\n\n\
             inspect it with `kubectl describe job/{}`, or raise `wait_timeout` on it",
            res.name
        );
        std::thread::sleep(interval);
    }
}

/// Waits at one gate.
fn wait_at_gate(
    rendered: &RenderedManifest,
    gate: &Gate,
    default_timeout: u64,
) -> anyhow::Result<()> {
    let timeout = gate.timeout.unwrap_or(default_timeout);
    match gate.wait {
        WaitFor::Complete => await_job(rendered, &gate.resource, timeout),
        WaitFor::Ready => await_rollout(rendered, &gate.resource, timeout),
    }
}

/// `a, b, c` for the resources of some phases.
fn names_in(phases: &[crate::render::Phase]) -> String {
    let all: Vec<String> = phases
        .iter()
        .flat_map(|p| p.resources.iter().map(ToString::to_string))
        .collect();
    if all.is_empty() {
        "nothing".to_string()
    } else {
        all.join(", ")
    }
}

/// Applies a render phase by phase, waiting at each gate before the next phase goes out.
///
/// If anything fails, says what had been applied and what had not, because with several phases "the
/// apply failed" leaves a cluster in a state the reader has to reconstruct.
fn apply_in_phases(rendered: &RenderedManifest, wait: WaitPolicy) -> anyhow::Result<()> {
    let total = rendered.phases.len();

    for (index, phase) in rendered.phases.iter().enumerate() {
        print_status(
            Tone::Info,
            "phase",
            &format!(
                "{}/{total}: {}",
                index + 1,
                names_in(std::slice::from_ref(phase))
            ),
        );

        let status = kubectl(rendered)
            .args(["apply", "-f", &phase.path])
            .status()?;
        if !status.success() {
            // kubectl applies a manifest resource by resource, so this phase may be partly applied:
            // say which phases are known to be complete, which one broke, and which never started.
            anyhow::bail!(
                "kubectl apply failed in phase {}/{total}\n\n\
                 applied: {}\nfailed, possibly partly applied: {}\nnot applied: {}",
                index + 1,
                names_in(&rendered.phases[..index]),
                names_in(std::slice::from_ref(phase)),
                names_in(&rendered.phases[index + 1..])
            );
        }

        if let Some(gate) = &phase.gate
            && let Err(err) = wait_at_gate(rendered, gate, wait.timeout_seconds)
        {
            // What this phase applied *was* applied; only what follows is held back.
            return Err(anyhow::anyhow!(
                "{err}\n\napplied: {}\nnot applied: {}",
                names_in(&rendered.phases[..=index]),
                names_in(&rendered.phases[index + 1..])
            ));
        }
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

    // Stopping at gates only makes sense when something is really applied and waiting is wanted; a dry
    // run or `--no-wait` applies the whole, dependency-ordered manifest in one go.
    if !dry && wait.enabled && !rendered.phases.is_empty() {
        apply_in_phases(&rendered, wait)?;
    } else {
        let mut cmd = kubectl(&rendered);
        cmd.arg("apply");
        if dry {
            cmd.arg("--dry-run=client");
        }
        let status = cmd.arg("-f").arg(&rendered.path).status()?;
        anyhow::ensure!(status.success(), "kubectl apply failed");
    }

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

/// What `kubectl get -o jsonpath` is asked to print per object, tab-separated: its kind, its name, the
/// kinds that own it, and its `spec.selector`.
///
/// Fields rather than `-o json` on purpose: the kinds queried include `secret`, and a prune has no
/// business reading Secret contents into memory just to decide what to delete.
const LISTING: &str = r#"{range .items[*]}{.kind}{"\t"}{.metadata.name}{"\t"}{.metadata.ownerReferences[*].kind}{"\t"}{.spec.selector}{"\n"}{end}"#;

/// Reads [`LISTING`] output into the resources riveter itself created, leaving out the ones something
/// else derived from them.
///
/// The cluster's own controllers copy a Service's or Ingress's labels onto what they generate, so the
/// `managed-by` selector alone also matches objects riveter never made and must never delete:
///
/// - anything with an owner (`Certificate`s that cert-manager creates for an Ingress, the
///   `EndpointSlice`s of a Service, `ReplicaSet`s and `Pod`s) - riveter's own objects have none;
/// - the `Endpoints` Kubernetes keeps for a Service that has a selector. These carry no owner, so
///   they are recognised by that Service being in the same listing. An `Endpoints` for a Service
///   *without* a selector is the hand-written kind riveter's `endpoints` template exists for, and
///   stays prunable.
#[must_use]
pub fn parse_live_listing(listing: &str) -> Vec<LiveResource> {
    struct Row<'a> {
        kind: String,
        name: &'a str,
        owned: bool,
        selector: &'a str,
    }

    let rows: Vec<Row<'_>> = listing
        .lines()
        .filter_map(|line| {
            let mut fields = line.split('\t');
            let kind = fields.next()?.trim();
            let name = fields.next()?.trim();
            if kind.is_empty() || name.is_empty() {
                return None;
            }
            Some(Row {
                kind: kind.to_lowercase(),
                name,
                owned: !fields.next().unwrap_or_default().trim().is_empty(),
                selector: fields.next().unwrap_or_default().trim(),
            })
        })
        .collect();

    let selecting_services: std::collections::HashSet<&str> = rows
        .iter()
        .filter(|r| r.kind == "service" && !r.selector.is_empty())
        .map(|r| r.name)
        .collect();

    rows.iter()
        .filter(|r| !r.owned)
        .filter(|r| !(r.kind == "endpoints" && selecting_services.contains(r.name)))
        .map(|r| LiveResource {
            kind: r.kind.clone(),
            name: r.name.to_string(),
        })
        .collect()
}

/// Asks the cluster what it holds for this environment.
///
/// Queries the kinds riveter can render, since a resource it never created is
/// not its to remove - see [`parse_live_listing`] for the objects that merely
/// look like it. `raw` resources are unreachable this way - they carry
/// whatever labels the overlay wrote - so they are never pruned.
fn live_resources(env: &str, rendered: &RenderedManifest) -> anyhow::Result<Vec<LiveResource>> {
    let kinds = crate::render::prunable_kinds().join(",");
    let selector = format!("{MANAGED_BY},env={env}");

    let mut cmd = kubectl(rendered);
    cmd.args([
        "get",
        &kinds,
        "-l",
        &selector,
        "-o",
        &format!("jsonpath={LISTING}"),
    ]);
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

    Ok(parse_live_listing(&String::from_utf8_lossy(&out.stdout)))
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

pub fn parse_scope_value(raw: &str) -> anyhow::Result<ResourceScope> {
    match raw.to_ascii_lowercase().as_str() {
        "mutable" => Ok(ResourceScope::Mutable),
        "immutable" => Ok(ResourceScope::Immutable),
        "all" => Ok(ResourceScope::All),
        _ => anyhow::bail!("invalid --scope value `{raw}` (expected mutable|immutable|all)"),
    }
}

/// Names what the command actually accepts, so the error carries the fix.
#[must_use]
pub fn unknown_option(arg: &str, allow_dry_run: bool) -> String {
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
