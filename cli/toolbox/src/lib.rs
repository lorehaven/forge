use anyhow::{Context, Result};
use crossterm::event::KeyCode;
use crossterm::style::Stylize;
use std::collections::HashMap;
use std::process::Command;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, Instant};

use semver::Version;

pub const REGISTRY_NAME: &str = "ennor";
pub const REGISTRY_INDEX: &str = "sparse+https://ennor.ddns.net/index/";

#[derive(Clone, Copy)]
pub struct MonitoredCrate {
    pub package: &'static str,
    pub binary: &'static str,
}

/// Every installable the workspace builds, in the order they are listed.
///
/// `forge-toolbox` is deliberately absent: it cannot replace its own running
/// binary from inside this list, so it reports on itself through
/// [`format_toolbox_note`] instead. The services under `docker/` are absent too - they
/// ship as images, and nothing installs them with cargo.
pub const MONITORED_CRATES: &[MonitoredCrate] = &[
    MonitoredCrate {
        package: "anvil",
        binary: "anvil",
    },
    MonitoredCrate {
        package: "conveyor-cli",
        binary: "conveyor",
    },
    MonitoredCrate {
        package: "foreman",
        binary: "foreman",
    },
    MonitoredCrate {
        package: "pulley",
        binary: "pulley",
    },
    MonitoredCrate {
        package: "riveter",
        binary: "riveter",
    },
    MonitoredCrate {
        package: "welder",
        binary: "welder",
    },
    MonitoredCrate {
        package: "warehouse-cli",
        binary: "warehouse",
    },
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrateStatus {
    pub package: &'static str,
    pub binary: &'static str,
    pub installed_version: Option<Version>,
    pub latest_version: Option<Version>,
    pub installable: bool,
    pub updatable: bool,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionRequest {
    pub package: String,
    pub installable: bool,
    pub installed: bool,
    pub updatable: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlannedAction {
    Install,
    Update,
}

pub struct TableWidths {
    pub package: usize,
    pub binary: usize,
    pub installed: usize,
    pub latest: usize,
    pub updatable: usize,
    pub action: usize,
}

pub fn content_widths(statuses: &[CrateStatus]) -> TableWidths {
    let mut package = char_width("package");
    let mut binary = char_width("binary");
    let mut installed = char_width("installed");
    let mut latest = char_width("latest");
    let mut updatable = char_width("updatable");
    let mut action = char_width("action");

    for status in statuses {
        package = package.max(char_width(status.package));
        binary = binary.max(char_width(status.binary));
        installed = installed.max(char_width(&display_installed(status)));
        latest = latest.max(char_width(&display_latest(status)));
        updatable = updatable.max(char_width(display_updatable(status)));
        action = action.max(char_width(action_label(status)));
    }

    TableWidths {
        package: package + 2,
        binary: binary + 2,
        installed: installed + 2,
        latest: latest + 2,
        updatable: updatable + 2,
        action: action + 2,
    }
}

pub fn shrink_widths_to_fit(widths: &mut TableWidths, term_width: usize) {
    let min_package = 7;
    let min_binary = 7;
    let min_installed = 12;
    let min_latest = 6;
    let min_updatable = 9;
    let min_action = 6;

    while widths.package
        + widths.binary
        + widths.installed
        + widths.latest
        + widths.updatable
        + widths.action
        + 10
        > term_width
    {
        if widths.package > min_package {
            widths.package -= 1;
            continue;
        }
        if widths.binary > min_binary {
            widths.binary -= 1;
            continue;
        }
        if widths.installed > min_installed {
            widths.installed -= 1;
            continue;
        }
        if widths.latest > min_latest {
            widths.latest -= 1;
            continue;
        }
        if widths.updatable > min_updatable {
            widths.updatable -= 1;
            continue;
        }
        if widths.action > min_action {
            widths.action -= 1;
            continue;
        }
        break;
    }
}

pub fn make_border(
    left: char,
    mid: char,
    right: char,
    marker_w: usize,
    widths: &TableWidths,
) -> String {
    let mut out = String::new();
    out.push(left);
    out.push_str(&"─".repeat(marker_w));
    out.push(mid);
    out.push_str(&"─".repeat(widths.package));
    out.push(mid);
    out.push_str(&"─".repeat(widths.binary));
    out.push(mid);
    out.push_str(&"─".repeat(widths.installed));
    out.push(mid);
    out.push_str(&"─".repeat(widths.latest));
    out.push(mid);
    out.push_str(&"─".repeat(widths.updatable));
    out.push(mid);
    out.push_str(&"─".repeat(widths.action));
    out.push(right);
    out
}

pub fn display_installed(status: &CrateStatus) -> String {
    match (&status.installed_version, status.installable) {
        (Some(v), _) => v.to_string(),
        (None, true) => "no (can install)".to_string(),
        (None, false) => "no".to_string(),
    }
}

pub fn display_latest(status: &CrateStatus) -> String {
    status
        .latest_version
        .as_ref()
        .map_or_else(|| "n/a".to_string(), |v| v.to_string())
}

pub fn display_updatable(status: &CrateStatus) -> &'static str {
    if status.updatable { "yes" } else { "no" }
}

pub fn char_width(value: &str) -> usize {
    value.chars().count()
}

pub fn fit_cell(value: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }

    let chars: Vec<char> = value.chars().collect();
    let mut out: String = chars.iter().take(width).collect();

    if chars.len() > width && width > 1 {
        out = chars.iter().take(width - 1).collect();
        out.push('~');
    }

    format!("{:<width$}", out, width = width)
}

pub fn action_label(status: &CrateStatus) -> &'static str {
    if status.installed_version.is_none() && status.installable {
        "install"
    } else if status.updatable {
        "update"
    } else {
        "-"
    }
}

pub fn action_request(status: &CrateStatus) -> ActionRequest {
    ActionRequest {
        package: status.package.to_string(),
        installable: status.installable,
        installed: status.installed_version.is_some(),
        updatable: status.updatable,
    }
}

pub fn planned_action(req: &ActionRequest) -> Option<PlannedAction> {
    if !req.installable {
        return None;
    }
    if !req.installed {
        return Some(PlannedAction::Install);
    }
    if req.updatable {
        return Some(PlannedAction::Update);
    }
    None
}

pub fn run_selected_action(req: ActionRequest) -> Result<String> {
    if !req.installable {
        return Ok(format!(
            "{} is not installable from registry {}",
            req.package, REGISTRY_NAME
        ));
    }

    if req.installed && !req.updatable {
        return Ok(format!("{} is already up to date", req.package));
    }

    let mut cmd = Command::new("cargo");
    cmd.arg("install")
        .arg(&req.package)
        .arg("--registry")
        .arg(REGISTRY_NAME);

    if req.installed {
        cmd.arg("--force");
    }

    let output = cmd
        .output()
        .with_context(|| format!("failed to run cargo install for {}", req.package))?;

    finish_action(&req, output)
}

/// Turns a finished `cargo install`'s output into the same success message or
/// error `run_selected_action` produces, split out so a fake `Output` (no
/// real process, no mutated cargo state) can exercise both branches in tests.
pub fn finish_action(req: &ActionRequest, output: std::process::Output) -> Result<String> {
    if output.status.success() {
        let msg = if req.installed {
            format!("{} updated", req.package)
        } else {
            format!("{} installed", req.package)
        };
        return Ok(msg);
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let detail = stderr
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("cargo install failed");

    anyhow::bail!("{}: {}", req.package, detail)
}

pub fn spinner_frames() -> &'static [&'static str] {
    &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]
}

/// Pure decision logic for a single crate's status, given its installed
/// version and the result of a (possibly failed) registry lookup. Split out
/// from the network-calling wrapper so the installable/updatable/error matrix
/// can be tested without shelling out to `cargo`.
pub fn build_status(
    package: &'static str,
    binary: &'static str,
    installed_version: Option<Version>,
    latest_result: Result<Option<Version>>,
) -> CrateStatus {
    match latest_result {
        Ok(latest_version) => {
            let installable = latest_version.is_some();
            let updatable = match (&installed_version, &latest_version) {
                (Some(current), Some(latest)) => latest > current,
                _ => false,
            };
            CrateStatus {
                package,
                binary,
                installed_version,
                latest_version,
                installable,
                updatable,
                error: None,
            }
        }
        Err(err) => CrateStatus {
            package,
            binary,
            installed_version,
            latest_version: None,
            installable: false,
            updatable: false,
            error: Some(err.to_string()),
        },
    }
}

pub fn collect_statuses(installed: &HashMap<String, Version>) -> Vec<CrateStatus> {
    MONITORED_CRATES
        .iter()
        .map(|item| {
            let installed_version = installed.get(item.package).cloned();
            let latest_result = fetch_latest_registry_version(item.package)
                .with_context(|| format!("Failed to fetch latest version for {}", item.package));
            build_status(item.package, item.binary, installed_version, latest_result)
        })
        .collect()
}

/// Pure formatting for the toolbox self-update banner, given the installed
/// and latest `forge-toolbox` versions (each independently optional, since
/// either lookup can fail or come up empty).
pub fn format_toolbox_note(installed: Option<Version>, latest: Option<Version>) -> String {
    match (installed, latest) {
        (Some(current), Some(latest)) if latest > current => format!(
            "note: forge-toolbox update available: {current} -> {latest}. run forge-toolbox self-update"
        ),
        (Some(_), Some(_)) => "note: forge-toolbox is up to date".to_string(),
        (None, Some(latest)) => {
            format!("note: forge-toolbox is not installed (latest {latest})")
        }
        _ => "note: could not determine latest forge-toolbox version from registry".to_string(),
    }
}

pub fn toolbox_note(installed: &HashMap<String, Version>) -> String {
    let installed_toolbox = installed.get("forge-toolbox").cloned();
    let latest_toolbox = fetch_latest_registry_version("forge-toolbox")
        .ok()
        .flatten();
    format_toolbox_note(installed_toolbox, latest_toolbox)
}

pub fn fetch_latest_registry_version(package: &str) -> Result<Option<Version>> {
    let output = Command::new("cargo")
        .arg("search")
        .arg(package)
        .arg("--limit")
        .arg("10")
        .arg("--registry")
        .arg(REGISTRY_NAME)
        .output()
        .with_context(|| format!("Failed to execute cargo search for package {package}"))?;

    parse_search_output(package, output)
}

/// The parsing half of [`fetch_latest_registry_version`], split out so a
/// fake `Output` (no real `cargo search`) can exercise the failure branch.
pub fn parse_search_output(package: &str, output: std::process::Output) -> Result<Option<Version>> {
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        anyhow::bail!(
            "cargo search failed for package {package} (registry {REGISTRY_NAME}): {stderr}"
        );
    }

    let stdout = String::from_utf8(output.stdout)
        .with_context(|| format!("Invalid UTF-8 output while searching package {package}"))?;

    for line in stdout.lines() {
        if let Some(version) = parse_search_line(package, line)? {
            return Ok(Some(version));
        }
    }

    Ok(None)
}

pub fn parse_search_line(package: &str, line: &str) -> Result<Option<Version>> {
    let prefix = format!("{package} = \"");
    let trimmed = line.trim();

    if !trimmed.starts_with(&prefix) {
        return Ok(None);
    }

    let rest = &trimmed[prefix.len()..];
    let end = rest.find('"').with_context(|| {
        format!("Could not parse version from cargo search output line: {line}")
    })?;

    let version = Version::parse(&rest[..end])
        .with_context(|| format!("Invalid semver in cargo search output line: {line}"))?;

    Ok(Some(version))
}

pub fn installed_versions() -> Result<HashMap<String, Version>> {
    let output = Command::new("cargo")
        .arg("install")
        .arg("--list")
        .output()
        .context("Failed to execute cargo install --list")?;

    parse_installed_output(output)
}

/// The parsing half of [`installed_versions`], split out so a fake `Output`
/// (no real `cargo install --list`) can exercise the failure branch.
pub fn parse_installed_output(output: std::process::Output) -> Result<HashMap<String, Version>> {
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        anyhow::bail!("cargo install --list failed: {stderr}");
    }

    let stdout =
        String::from_utf8(output.stdout).context("Invalid UTF-8 from cargo install --list")?;

    Ok(parse_installed_list(&stdout))
}

/// Pure parser for `cargo install --list` output, e.g.:
///
/// ```text
/// anvil v0.1.22:
///     anvil
/// riveter v0.2.3 (registry `ennor`):
///     riveter
/// ```
///
/// Split out from [`installed_versions`] so the line-parsing logic can be
/// tested against fixture output without a real cargo install directory.
pub fn parse_installed_list(stdout: &str) -> HashMap<String, Version> {
    let mut versions = HashMap::new();

    for line in stdout.lines() {
        if line.starts_with(' ') || !line.ends_with(':') {
            continue;
        }

        let mut parts = line.split_whitespace();
        let Some(package) = parts.next() else {
            continue;
        };

        let Some(version_raw) = parts.next() else {
            continue;
        };

        let version_trimmed = version_raw.trim_start_matches('v').trim_end_matches(':');
        if let Ok(version) = Version::parse(version_trimmed) {
            versions.insert(package.to_string(), version);
        }
    }

    versions
}

/// The interactive monitor's state: the currently selected row, the last
/// status snapshot, the toolbox's own update banner, and the latest status
/// line message.
pub struct App {
    pub selected: usize,
    pub statuses: Vec<CrateStatus>,
    pub toolbox_note: String,
    pub message: String,
}

/// Re-fetches installed/latest versions and rebuilds [`App`], clamping
/// `selected` to the new status list so a shrinking list can't leave it
/// pointing past the end.
pub fn refresh_app_state(selected: usize, message: &str) -> Result<App> {
    let installed = installed_versions()?;
    let statuses = collect_statuses(&installed);
    let note = toolbox_note(&installed);

    let safe_selected = if statuses.is_empty() {
        0
    } else {
        selected.min(statuses.len() - 1)
    };

    Ok(App {
        selected: safe_selected,
        statuses,
        toolbox_note: note,
        message: message.to_string(),
    })
}

/// What a key press should do, decided independently of any real terminal so
/// it can be tested with synthetic [`KeyCode`] values.
#[derive(Debug, PartialEq, Eq)]
pub enum KeyOutcome {
    MoveUp,
    MoveDown,
    Quit,
    Refresh,
    RunAction(ActionRequest),
    /// A refresh or action key was pressed while one was already running.
    Busy,
    Ignore,
}

pub fn handle_key(
    code: KeyCode,
    selected: usize,
    count: usize,
    in_flight: bool,
    statuses: &[CrateStatus],
) -> KeyOutcome {
    match code {
        KeyCode::Up => KeyOutcome::MoveUp,
        KeyCode::Down if selected + 1 < count => KeyOutcome::MoveDown,
        KeyCode::Char('q') => KeyOutcome::Quit,
        KeyCode::Char('r') => {
            if in_flight {
                KeyOutcome::Busy
            } else {
                KeyOutcome::Refresh
            }
        }
        KeyCode::Enter => {
            if in_flight {
                return KeyOutcome::Busy;
            }
            match statuses.get(selected) {
                Some(status) => KeyOutcome::RunAction(action_request(status)),
                None => KeyOutcome::Ignore,
            }
        }
        _ => KeyOutcome::Ignore,
    }
}

/// A spawned install/update action, polled from the event loop.
pub struct InFlightAction {
    pub rx: Receiver<Result<String, String>>,
    pub selected: usize,
    pub spinner_idx: usize,
    pub label: String,
    pub last_tick: Instant,
}

#[derive(Debug, PartialEq, Eq)]
pub enum PollOutcome {
    /// Still running, nothing changed.
    Pending,
    /// Still running, the spinner advanced - redraw.
    Tick,
    Done {
        selected: usize,
        message: String,
    },
}

/// Advances an in-flight action's spinner or resolves it, given how often the
/// spinner should tick. Split out from the event loop so the channel/timer
/// state machine can be tested without a real terminal.
pub fn poll_in_flight(job: &mut InFlightAction, tick_every: Duration) -> PollOutcome {
    match job.rx.try_recv() {
        Ok(result) => {
            let message = match result {
                Ok(msg) => msg,
                Err(err) => format!("action failed: {err}"),
            };
            PollOutcome::Done {
                selected: job.selected,
                message,
            }
        }
        Err(TryRecvError::Empty) => {
            if job.last_tick.elapsed() >= tick_every {
                job.spinner_idx = (job.spinner_idx + 1) % spinner_frames().len();
                job.last_tick = Instant::now();
                PollOutcome::Tick
            } else {
                PollOutcome::Pending
            }
        }
        Err(TryRecvError::Disconnected) => PollOutcome::Done {
            selected: job.selected,
            message: "action failed: worker disconnected".to_string(),
        },
    }
}

/// What to do about a [`KeyOutcome::RunAction`]: either it already resolved
/// synchronously (not installable, or already up to date - no `cargo`
/// invocation needed), or it needs to run in the background with the given
/// spinner label.
#[derive(Debug, PartialEq, Eq)]
pub enum ActionDispatch {
    Done(String),
    Spawn { label: String },
}

/// Decides what running the given action should do, resolving the
/// no-op cases (not installable, already up to date) immediately since they
/// never shell out. Split out from the event loop so the label formatting
/// and no-op short-circuiting can be tested without spawning a thread.
pub fn dispatch_action(req: ActionRequest) -> ActionDispatch {
    match planned_action(&req) {
        None => {
            let message =
                run_selected_action(req).unwrap_or_else(|err| format!("action failed: {err}"));
            ActionDispatch::Done(message)
        }
        Some(PlannedAction::Install) => ActionDispatch::Spawn {
            label: format!("running {} (installing)", req.package),
        },
        Some(PlannedAction::Update) => ActionDispatch::Spawn {
            label: format!("running {} (updating)", req.package),
        },
    }
}

/// What the event loop should do after a tick of the in-flight action's
/// spinner/completion channel.
#[derive(Debug, PartialEq, Eq)]
pub enum TickEffect {
    None,
    Redraw,
    Refresh { selected: usize, message: String },
}

/// Advances (or resolves) `in_flight` and reports what the caller must do
/// about it. Split out from the event loop, alongside [`poll_in_flight`], so
/// the "nothing running" / "resolved, clear it and reset it to None" wiring
/// is tested too, not just the channel state machine itself.
pub fn on_tick(in_flight: &mut Option<InFlightAction>, tick_every: Duration) -> TickEffect {
    let Some(job) = in_flight.as_mut() else {
        return TickEffect::None;
    };

    match poll_in_flight(job, tick_every) {
        PollOutcome::Done { selected, message } => {
            *in_flight = None;
            TickEffect::Refresh { selected, message }
        }
        PollOutcome::Tick => TickEffect::Redraw,
        PollOutcome::Pending => TickEffect::None,
    }
}

/// The spinner + label line to show in place of the app's last status
/// message while an action is running, or `None` when nothing is running.
pub fn transient_status(in_flight: &Option<InFlightAction>) -> Option<String> {
    in_flight
        .as_ref()
        .map(|job| format!("{} {}", spinner_frames()[job.spinner_idx], job.label))
}

/// What the event loop should do after a key press: everything from moving
/// the selection to kicking off a background install/update.
#[derive(Debug, PartialEq, Eq)]
pub enum KeyEffect {
    None,
    Redraw,
    Quit,
    Refresh {
        selected: usize,
        message: String,
    },
    Spawn {
        selected: usize,
        label: String,
        req: ActionRequest,
    },
}

/// Applies a key press to `app` (mutating its selection in place) and
/// reports what the caller must do about it. Combines [`handle_key`] and
/// [`dispatch_action`] with their resulting state changes so the event loop
/// itself only has to perform I/O (spawning the background thread,
/// re-fetching status), not decide anything.
pub fn on_key(app: &mut App, in_flight: bool, code: KeyCode) -> KeyEffect {
    match handle_key(
        code,
        app.selected,
        app.statuses.len(),
        in_flight,
        &app.statuses,
    ) {
        KeyOutcome::MoveUp => {
            app.selected = app.selected.saturating_sub(1);
            KeyEffect::Redraw
        }
        KeyOutcome::MoveDown => {
            app.selected += 1;
            KeyEffect::Redraw
        }
        KeyOutcome::Quit => KeyEffect::Quit,
        KeyOutcome::Refresh => KeyEffect::Refresh {
            selected: app.selected,
            message: "Status refreshed".to_string(),
        },
        KeyOutcome::Busy | KeyOutcome::Ignore => KeyEffect::None,
        KeyOutcome::RunAction(req) => {
            let selected = app.selected;
            match dispatch_action(req.clone()) {
                ActionDispatch::Done(message) => KeyEffect::Refresh { selected, message },
                ActionDispatch::Spawn { label } => KeyEffect::Spawn {
                    selected,
                    label,
                    req,
                },
            }
        }
    }
}

/// Renders the monitor screen as a list of already-styled lines, one per
/// terminal row. Pure string building - split out from the event loop so it
/// can be tested without a real terminal (crossterm's `Stylize` just wraps
/// ANSI codes around the text, it doesn't need a tty).
pub fn render(app: &App, transient_status: Option<&str>, term_width: usize) -> Vec<String> {
    let usable_width = term_width.saturating_sub(1);
    let mut widths = content_widths(&app.statuses);
    shrink_widths_to_fit(&mut widths, usable_width);
    let row_len = widths.package
        + widths.binary
        + widths.installed
        + widths.latest
        + widths.updatable
        + widths.action
        + 10;
    let sep_len = row_len.min(usable_width);

    let mut lines = Vec::new();

    lines.push(format!("{}", "Forge Toolbox Status".bold().underlined()));
    lines.push(String::new());
    lines.push(format!(
        "{} {} {}",
        "registry:".dark_grey(),
        REGISTRY_NAME.cyan().bold(),
        format!("({REGISTRY_INDEX})").dark_grey()
    ));
    lines.push(format!("{}", app.toolbox_note.as_str().yellow()));
    lines.push(format!(
        "{} {}",
        "controls:".dark_grey(),
        "↑/↓ move, Enter install/update, r refresh, q quit".cyan()
    ));
    lines.push(String::new());

    let top = fit_cell(&make_border('┌', '┬', '┐', 2, &widths), sep_len);
    let mid = fit_cell(&make_border('├', '┼', '┤', 2, &widths), sep_len);
    let bottom = fit_cell(&make_border('└', '┴', '┘', 2, &widths), sep_len);

    lines.push(format!("{}", top.dark_cyan()));
    lines.push(format!(
        "│{}│{}│{}│{}│{}│{}│{}│",
        fit_cell("", 2).dark_cyan(),
        fit_cell("package", widths.package).bold().white(),
        fit_cell("binary", widths.binary).bold().white(),
        fit_cell("installed", widths.installed).bold().white(),
        fit_cell("latest", widths.latest).bold().white(),
        fit_cell("updatable", widths.updatable).bold().white(),
        fit_cell("action", widths.action).bold().white(),
    ));
    lines.push(format!("{}", mid.dark_cyan()));

    for (idx, status) in app.statuses.iter().enumerate() {
        let installed = display_installed(status);
        let latest = display_latest(status);
        let updatable = display_updatable(status);
        let action = action_label(status);
        let marker = if idx == app.selected {
            fit_cell(">", 2).green().bold().to_string()
        } else {
            fit_cell(" ", 2).dark_grey().to_string()
        };

        let installed_cell = match (&status.installed_version, status.installable) {
            (Some(_), _) => fit_cell(&installed, widths.installed).green().to_string(),
            (None, true) => fit_cell(&installed, widths.installed).yellow().to_string(),
            (None, false) => fit_cell(&installed, widths.installed)
                .dark_grey()
                .to_string(),
        };
        let latest_cell = if status.latest_version.is_some() {
            fit_cell(&latest, widths.latest).cyan().to_string()
        } else {
            fit_cell(&latest, widths.latest).dark_grey().to_string()
        };
        let updatable_cell = if status.updatable {
            fit_cell(updatable, widths.updatable)
                .yellow()
                .bold()
                .to_string()
        } else {
            fit_cell(updatable, widths.updatable)
                .dark_grey()
                .to_string()
        };
        let action_cell = match action {
            "install" => fit_cell(action, widths.action).green().bold().to_string(),
            "update" => fit_cell(action, widths.action).yellow().bold().to_string(),
            _ => fit_cell(action, widths.action).dark_grey().to_string(),
        };

        lines.push(format!(
            "│{}│{}│{}│{}│{}│{}│{}│",
            marker,
            fit_cell(status.package, widths.package).white(),
            fit_cell(status.binary, widths.binary).white(),
            installed_cell,
            latest_cell,
            updatable_cell,
            action_cell,
        ));

        if let Some(error) = &status.error {
            lines.push(format!("{} {}", "warn:".red().bold(), error.as_str().red()));
        }
    }
    lines.push(format!("{}", bottom.dark_cyan()));

    lines.push(String::new());
    let status_text = transient_status.unwrap_or(app.message.as_str());
    lines.push(format!("{} {}", "status:".dark_grey(), status_text.white()));

    lines
}
