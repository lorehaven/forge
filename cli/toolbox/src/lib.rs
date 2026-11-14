use forge_tui::widgets::{self, ButtonKind};
use forge_tui::{ColorLevel, Line, Style, Theme, Tone, fill_screen};
use quench_cli::keys::KeyCode;
use quench_core::{Context, Result};
use std::collections::HashMap;
use std::hash::BuildHasher;
use std::process::Command;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, Instant};

use quench_core::version::Version;

pub const REGISTRY_NAME: &str = "ennor";
pub const REGISTRY_INDEX: &str = "sparse+https://ennor.ddns.net/warehouse/index/";

#[derive(Clone, Copy, Debug)]
pub struct MonitoredCrate {
    pub package: &'static str,
    pub binary: &'static str,
}

/// The installables toolbox falls back to when the registry cannot be asked which crates install a binary
/// ([`discover_tools`]).
///
/// `forge-toolbox` is deliberately absent: it cannot replace its own running
/// binary from inside this list, so it reports on itself through
/// [`format_toolbox_note`] instead. The services under `services/` are absent too - they
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
        package: "loupe",
        binary: "loupe",
    },
    MonitoredCrate {
        package: "welder",
        binary: "welder",
    },
    MonitoredCrate {
        package: "warehouse-cli",
        binary: "warehouse",
    },
    // From the quench workspace, published to the same registry: the `quench` command that makes and runs apps.
    MonitoredCrate {
        package: "quench-tool",
        binary: "quench",
    },
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrateStatus {
    pub package: String,
    pub binary: String,
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

/// Column widths of the tools table, each including its two-column gutter. A width of 0 means the column is hidden.
#[derive(Debug)]
pub struct TableWidths {
    pub package: usize,
    pub binary: usize,
    pub installed: usize,
    pub latest: usize,
    pub status: usize,
    pub action: usize,
}

/// Columns before the first cell: the selection bar and the space after it.
const MARKER_WIDTH: usize = 2;
/// Padding the badge and button widgets put around their text.
const BADGE_CHROME: usize = 4;
const BUTTON_CHROME: usize = 2;

#[must_use]
pub fn content_widths(statuses: &[CrateStatus]) -> TableWidths {
    let mut package = char_width("Package");
    let mut binary = char_width("Binary");
    let mut installed = char_width("Installed");
    let mut latest = char_width("Latest");
    let mut status_w = char_width("Status");
    let mut action = char_width("Action");

    for status in statuses {
        package = package.max(char_width(&status.package));
        binary = binary.max(char_width(&status.binary));
        installed = installed.max(char_width(&display_installed(status)));
        latest = latest.max(char_width(&display_latest(status)));
        status_w = status_w.max(char_width(status_kind(status).1) + BADGE_CHROME);
        action = action.max(char_width(action_text(status)) + BUTTON_CHROME);
    }

    TableWidths {
        package: package + 2,
        binary: binary + 2,
        installed: installed + 2,
        latest: latest + 2,
        status: status_w + 2,
        action: action + 2,
    }
}

impl TableWidths {
    const fn total(&self) -> usize {
        MARKER_WIDTH
            + self.package
            + self.binary
            + self.installed
            + self.latest
            + self.status
            + self.action
    }
}

/// Narrows the columns until the table fits `term_width` columns.
///
/// Columns shrink towards a floor, widest-first in a fixed order; if that is still too wide the binary column and then
/// the latest-version column are hidden (set to 0), the way the web table drops columns on a phone.
pub const fn shrink_widths_to_fit(widths: &mut TableWidths, term_width: usize) {
    let min_package = 9;
    let min_binary = 9;
    let min_installed = 11;
    let min_latest = 8;
    let min_status = 14;
    let min_action = 9;

    while widths.total() > term_width {
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
        if widths.status > min_status {
            widths.status -= 1;
            continue;
        }
        if widths.action > min_action {
            widths.action -= 1;
            continue;
        }
        if widths.binary > 0 {
            widths.binary = 0;
            continue;
        }
        if widths.latest > 0 {
            widths.latest = 0;
            continue;
        }
        break;
    }
}

#[must_use]
pub fn display_installed(status: &CrateStatus) -> String {
    status
        .installed_version
        .as_ref()
        .map_or_else(|| "—".to_string(), std::string::ToString::to_string)
}

#[must_use]
pub fn display_latest(status: &CrateStatus) -> String {
    status
        .latest_version
        .as_ref()
        .map_or_else(|| "—".to_string(), std::string::ToString::to_string)
}

/// The badge of the status column: its tone and text.
#[must_use]
pub const fn status_kind(status: &CrateStatus) -> (Tone, &'static str) {
    if status.error.is_some() {
        (Tone::Error, "Registry error")
    } else if status.installed_version.is_none() {
        if status.installable {
            (Tone::Gray, "Not installed")
        } else {
            (Tone::Gray, "Unavailable")
        }
    } else if status.updatable {
        (Tone::Warning, "Update available")
    } else if status.latest_version.is_none() {
        (Tone::Gray, "Not in registry")
    } else {
        (Tone::Success, "Up to date")
    }
}

/// The text of the action button, `—` when Enter would do nothing.
#[must_use]
pub const fn action_text(status: &CrateStatus) -> &'static str {
    match action_label(status).as_bytes() {
        b"install" => "Install",
        b"update" => "Update",
        _ => "—",
    }
}

/// The tone of the toolbox self-update banner (see [`format_toolbox_note`]).
#[must_use]
pub fn note_tone(note: &str) -> Tone {
    if note.contains("update available") {
        Tone::Warning
    } else if note.contains("up to date") {
        Tone::Success
    } else if note.contains("not installed") {
        Tone::Info
    } else {
        Tone::Gray
    }
}

/// The tone of the status message at the bottom: failures are errors, a finished install or update is a success.
#[must_use]
pub fn message_tone(message: &str) -> Tone {
    if message.contains("failed") || message.contains("not installable") {
        Tone::Error
    } else if message.ends_with("installed") || message.ends_with("updated") {
        Tone::Success
    } else {
        Tone::Gray
    }
}

#[must_use]
pub fn char_width(value: &str) -> usize {
    value.chars().count()
}

#[must_use]
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

    format!("{out:<width$}")
}

#[must_use]
pub const fn action_label(status: &CrateStatus) -> &'static str {
    if status.installed_version.is_none() && status.installable {
        "install"
    } else if status.updatable {
        "update"
    } else {
        "-"
    }
}

#[must_use]
pub fn action_request(status: &CrateStatus) -> ActionRequest {
    ActionRequest {
        package: status.package.clone(),
        installable: status.installable,
        installed: status.installed_version.is_some(),
        updatable: status.updatable,
    }
}

#[must_use]
pub const fn planned_action(req: &ActionRequest) -> Option<PlannedAction> {
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

pub fn run_selected_action(req: &ActionRequest) -> Result<String> {
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

    finish_action(req, &output)
}

/// Turns a finished `cargo install`'s output into the same success message or
/// error `run_selected_action` produces.
///
/// Split out so a fake `Output` (no
/// real process, no mutated cargo state) can exercise both branches in tests.
pub fn finish_action(req: &ActionRequest, output: &std::process::Output) -> Result<String> {
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

    quench_core::bail!("{}: {}", req.package, detail)
}

#[must_use]
pub const fn spinner_frames() -> &'static [&'static str] {
    &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]
}

/// Pure decision logic for a single crate's status, given its installed
/// version and the result of a (possibly failed) registry lookup.
///
/// Split out
/// from the network-calling wrapper so the installable/updatable/error matrix
/// can be tested without shelling out to `cargo`.
#[must_use]
pub fn build_status(
    package: &str,
    binary: &str,
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
                package: package.to_string(),
                binary: binary.to_string(),
                installed_version,
                latest_version,
                installable,
                updatable,
                error: None,
            }
        }
        Err(err) => CrateStatus {
            package: package.to_string(),
            binary: binary.to_string(),
            installed_version,
            latest_version: None,
            installable: false,
            updatable: false,
            error: Some(err.to_string()),
        },
    }
}

#[must_use]
pub fn collect_statuses<S: BuildHasher>(
    installed: &HashMap<String, Version, S>,
) -> Vec<CrateStatus> {
    let discovered = discover_tools();
    let tools: Vec<(String, String)> = discovered.unwrap_or_else(|_| {
        MONITORED_CRATES
            .iter()
            .map(|item| (item.package.to_string(), item.binary.to_string()))
            .collect()
    });
    tools
        .iter()
        .map(|(package, binary)| {
            let installed_version = installed.get(package).cloned();
            let latest_result = fetch_latest_registry_version(package)
                .with_context(|| format!("Failed to fetch latest version for {package}"));
            build_status(package, binary, installed_version, latest_result)
        })
        .collect()
}

/// The registry's JSON API root, derived from [`REGISTRY_INDEX`]
/// (`sparse+https://host/warehouse/index/` -> `https://host/warehouse/api/v1/crates`).
#[must_use]
pub fn tools_url() -> String {
    let base = REGISTRY_INDEX.trim_start_matches("sparse+");
    let root = base.trim_end_matches('/');
    let root = root.strip_suffix("/index").unwrap_or(root);
    format!("{root}/api/v1/crates?bins=true&per_page=100")
}

/// Asks the registry which crates install a binary, as `(package, binary)` sorted by package.
///
/// Only command-line tools are kept, see [`is_cli_tool`].
///
/// # Errors
/// The registry cannot be reached, answers an error, or sends something that is not a crate list.
pub fn discover_tools() -> Result<Vec<(String, String)>> {
    let client = quench_client::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .context("Failed to build the HTTP client")?;
    let response = client
        .get(tools_url())
        .send()
        .context("Failed to ask the registry for its tools")?;
    if !response.status().is_success() {
        quench_core::bail!("registry tool listing failed: HTTP {}", response.status());
    }
    let body = response
        .bytes()
        .context("Failed to read the registry tool listing")?;
    parse_tools(&String::from_utf8_lossy(&body))
}

/// Whether a registry crate is a command-line tool toolbox should offer.
///
/// It says so with the
/// `command-line-utilities` or `development-tools` category, and is not a `-service` (those ship as images) or
/// `forge-toolbox` itself (it reports on itself through [`format_toolbox_note`]). A GUI, fixture or library-with-a-demo
/// crate carries neither category.
#[must_use]
pub fn is_cli_tool(name: &str, categories: &[String]) -> bool {
    name != "forge-toolbox"
        && !name.ends_with("-service")
        && categories
            .iter()
            .any(|c| c == "command-line-utilities" || c == "development-tools")
}

/// The parsing half of [`discover_tools`]: the `crates` of a search response that list a binary.
///
/// # Errors
/// The text is not a search response, or lists no installable tool at all.
pub fn parse_tools(json: &str) -> Result<Vec<(String, String)>> {
    #[derive(quench_serde::Deserialize)]
    struct Listing {
        crates: Vec<Item>,
    }
    #[derive(quench_serde::Deserialize)]
    struct Item {
        name: String,
        #[serde(default)]
        bins: Vec<String>,
        #[serde(default)]
        categories: Vec<String>,
    }
    let listing: Listing =
        quench_serde::json::from_str(json).context("Registry tool listing is not a crate list")?;
    let mut tools: Vec<(String, String)> = listing
        .crates
        .into_iter()
        .filter(|item| is_cli_tool(&item.name, &item.categories))
        .filter_map(|item| item.bins.into_iter().next().map(|bin| (item.name, bin)))
        .collect();
    if tools.is_empty() {
        quench_core::bail!("Registry lists no installable tools");
    }
    tools.sort();
    Ok(tools)
}

/// What `forge-toolbox self-update` should do, given the installed and latest versions.
#[derive(Debug, PartialEq, Eq)]
pub enum SelfUpdate {
    /// Already at (or ahead of) the registry's newest version.
    UpToDate(Version),
    /// Install this version; also taken when toolbox is not in `cargo install --list` (run from a build).
    Install(Version),
    /// The registry has no `forge-toolbox`.
    NotPublished,
}

#[must_use]
pub fn plan_self_update(installed: Option<&Version>, latest: Option<&Version>) -> SelfUpdate {
    match (installed, latest) {
        (_, None) => SelfUpdate::NotPublished,
        (Some(current), Some(latest)) if latest <= current => SelfUpdate::UpToDate(current.clone()),
        (_, Some(latest)) => SelfUpdate::Install(latest.clone()),
    }
}

/// The command `self-update` runs: replaces the installed binary (a running executable can be replaced on Linux).
#[must_use]
pub fn self_update_command() -> Command {
    let mut cmd = Command::new("cargo");
    cmd.args([
        "install",
        "forge-toolbox",
        "--registry",
        REGISTRY_NAME,
        "--force",
    ]);
    cmd
}

/// Pure formatting for the toolbox self-update banner, given the installed
/// and latest `forge-toolbox` versions (each independently optional, since
/// either lookup can fail or come up empty).
#[must_use]
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

#[must_use]
pub fn toolbox_note<S: BuildHasher>(installed: &HashMap<String, Version, S>) -> String {
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
        quench_core::bail!(
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
        quench_core::bail!("cargo install --list failed: {stderr}");
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
#[must_use]
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
#[derive(Debug)]
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

#[must_use]
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
            statuses.get(selected).map_or(KeyOutcome::Ignore, |status| {
                KeyOutcome::RunAction(action_request(status))
            })
        }
        _ => KeyOutcome::Ignore,
    }
}

/// A spawned install/update action, polled from the event loop.
#[derive(Debug)]
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

/// What to do about a [`KeyOutcome::RunAction`].
///
/// Either it already resolved synchronously (not installable, or already up to date - no `cargo`
/// invocation needed), or it needs to run in the background with the given
/// spinner label.
#[derive(Debug, PartialEq, Eq)]
pub enum ActionDispatch {
    Done(String),
    Spawn { label: String },
}

/// Decides what running the given action should do.
///
/// It resolves the no-op cases (not installable, already up to date)
/// immediately since they never shell out. Split out from the event loop so the label formatting
/// and no-op short-circuiting can be tested without spawning a thread.
#[must_use]
pub fn dispatch_action(req: &ActionRequest) -> ActionDispatch {
    match planned_action(req) {
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
/// about it.
///
/// Split out from the event loop, alongside [`poll_in_flight`], so
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
#[must_use]
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
/// reports what the caller must do about it.
///
/// Combines [`handle_key`] and
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
            match dispatch_action(&req) {
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

/// The bytes that paint `rows` over whatever is on the screen, without clearing it first.
///
/// The cursor goes home, each row is written over the old one and followed by `erase` (a background escape, or empty)
/// and "erase to end of line", and everything below the last row is erased, all in one write.
/// Clearing the screen before each frame is what made the old screen blink on every spinner tick. Rows should be one
/// column narrower than the terminal: the erase fills the last column, so nothing is ever written there and no terminal
/// is left in its "pending wrap" state.
#[must_use]
pub fn frame(rows: &[String], erase: &str) -> String {
    let mut out = String::from("\x1b[H");
    let tail = format!("{erase}\x1b[K\x1b[0m\r\n");
    out.push_str(&rows.join(&tail));
    out.push_str(&tail.replace("\r\n", ""));
    out.push_str(erase);
    out.push_str("\x1b[J\x1b[0m");
    out
}

/// Renders the monitor screen with the theme and colour level of the environment (see [`render_with`]); the rows are
/// not padded down to the terminal height.
#[must_use]
pub fn render(app: &App, transient_status: Option<&str>, term_width: usize) -> Vec<String> {
    render_with(
        app,
        transient_status,
        term_width,
        0,
        &Theme::from_env(),
        ColorLevel::detect(),
    )
}

/// Renders the monitor screen as one string per terminal row, styled like the forge web apps.
///
/// A page background, a header with the version tag, the self-update banner as an alert, the tools in a card with
/// status badges and action buttons, the keys as chips and the last message as an alert. Pure string building, so it can be tested without a terminal. With a `term_height` the rows are painted down to
/// the bottom of the screen; 0 leaves the row count alone.
#[must_use]
pub fn render_with(
    app: &App,
    transient_status: Option<&str>,
    term_width: usize,
    term_height: usize,
    theme: &Theme,
    level: ColorLevel,
) -> Vec<String> {
    let page = page_lines(app, transient_status, term_width, term_height, theme);
    let rows = if level == ColorLevel::None {
        page
    } else {
        fill_screen(page, term_width, term_height, theme.bg)
    };
    rows.iter().map(|line| line.render(level)).collect()
}

/// The widest the content grows; wider terminals centre it, as the web page does with `--fui-content-max`.
const CONTENT_MAX: usize = 120;

/// Rows of the page that are not tool rows: header block, banner, card chrome, key hints and the message.
const PAGE_CHROME_ROWS: usize = 16;

fn page_lines(
    app: &App,
    transient_status: Option<&str>,
    term_width: usize,
    term_height: usize,
    theme: &Theme,
) -> Vec<Line> {
    let margin = if term_width >= 60 { 2 } else { 1 };
    let content = term_width.saturating_sub(margin * 2).clamp(1, CONTENT_MAX);
    let margin = margin.max(term_width.saturating_sub(content) / 2);
    let indent = |line: Line| Line::new().space(margin, Style::new()).append(line);
    let blank = Line::new();

    let text = Style::new().fg(theme.text);
    let tertiary = Style::new().fg(theme.text_tertiary);

    let mut page: Vec<Line> = vec![blank.clone()];

    // Page header: title, version tag, subtitle.
    page.push(indent(
        Line::styled("Forge Toolbox", text.bold())
            .space(2, Style::new())
            .append(widgets::tag(
                theme,
                Tone::Brand,
                concat!("v", env!("CARGO_PKG_VERSION")),
            )),
    ));
    page.push(indent(
        Line::styled(
            "Monitor and install the forge command-line tools.",
            tertiary,
        )
        .truncate(content),
    ));
    page.push(indent(
        Line::styled("Registry ", tertiary)
            .push(
                REGISTRY_NAME,
                Style::new().fg(theme.soft(Tone::Brand).text).bold(),
            )
            .push(format!("  {REGISTRY_INDEX}"), tertiary)
            .truncate(content),
    ));
    page.push(blank.clone());

    // Self-update banner.
    let note = app.toolbox_note.trim_start_matches("note: ");
    page.push(indent(widgets::alert(
        theme,
        note_tone(&app.toolbox_note),
        note,
        content,
    )));
    page.push(blank.clone());

    // The tools card.
    page.extend(
        tools_card(app, theme, content, tool_rows_available(term_height))
            .into_iter()
            .map(&indent),
    );
    page.push(blank.clone());

    // Key hints.
    let gap = Style::new();
    let hints = widgets::hint(theme, "↑↓", "Navigate")
        .space(3, gap)
        .append(widgets::hint(theme, "↵", "Install / update"))
        .space(3, gap)
        .append(widgets::hint(theme, "r", "Refresh"))
        .space(3, gap)
        .append(widgets::hint(theme, "q", "Quit"));
    page.push(indent(hints.truncate(content)));
    page.push(blank);

    // Last message, or the running action with its spinner.
    let status_line = transient_status.map_or_else(
        || widgets::alert(theme, message_tone(&app.message), &app.message, content),
        |busy| {
            let (icon, label) = busy.split_once(' ').unwrap_or(("…", busy));
            widgets::alert_with_icon(theme, Tone::Brand, icon, label, content)
        },
    );
    page.push(indent(status_line));
    page
}

/// How many table rows fit on a screen of `term_height` rows (`None` when the height is unknown, 0).
fn tool_rows_available(term_height: usize) -> Option<usize> {
    (term_height != 0).then(|| term_height.saturating_sub(PAGE_CHROME_ROWS).max(3))
}

/// Table rows one tool takes: its own, plus a line for its error.
const fn row_count(status: &CrateStatus) -> usize {
    if status.error.is_some() { 2 } else { 1 }
}

/// The tools shown when only `capacity` table rows fit: the range that ends at or after the selected tool, as far
/// up as needed to keep it in view, then filled downwards.
#[must_use]
pub fn visible_range(
    statuses: &[CrateStatus],
    selected: usize,
    capacity: usize,
) -> std::ops::Range<usize> {
    if statuses.is_empty() {
        return 0..0;
    }
    let selected = selected.min(statuses.len() - 1);
    let mut start = 0;
    let mut used: usize = statuses[..=selected].iter().map(row_count).sum();
    while used > capacity && start < selected {
        used -= row_count(&statuses[start]);
        start += 1;
    }
    let mut end = selected + 1;
    while end < statuses.len() && used + row_count(&statuses[end]) <= capacity {
        used += row_count(&statuses[end]);
        end += 1;
    }
    start..end
}

/// The card with the tools table, `width` columns wide including its border, showing as many rows as `capacity` allows.
fn tools_card(app: &App, theme: &Theme, width: usize, capacity: Option<usize>) -> Vec<Line> {
    let inner = widgets::card_inner(width);
    let updates = app.statuses.iter().filter(|s| s.updatable).count();
    let shown = capacity.map_or(0..app.statuses.len(), |rows| {
        visible_range(&app.statuses, app.selected, rows)
    });

    let count = if shown.len() < app.statuses.len() {
        format!(
            "  {}–{} of {}",
            shown.start + 1,
            shown.end,
            app.statuses.len()
        )
    } else {
        let noun = if app.statuses.len() == 1 {
            "package"
        } else {
            "packages"
        };
        format!("  {} {noun}", app.statuses.len())
    };
    let mut header = Line::styled(" Tools", Style::new().fg(theme.text).bold())
        .push(count, Style::new().fg(theme.text_tertiary));
    if updates > 0 {
        let label = format!(
            "{updates} {}",
            if updates == 1 { "update" } else { "updates" }
        );
        header = header
            .space(2, Style::new())
            .append(widgets::badge(theme, Tone::Warning, &label));
    }

    let mut widths = content_widths(&app.statuses);
    shrink_widths_to_fit(&mut widths, inner);

    let mut body = vec![column_header(theme, &widths, inner)];
    if app.statuses.is_empty() {
        body.push(Line::styled(
            "  No tools found in the registry.",
            Style::new().fg(theme.text_tertiary),
        ));
    }
    for index in shown {
        let status = &app.statuses[index];
        let selected = index == app.selected;
        body.push(tool_row(theme, &widths, status, selected, inner));
        if let Some(error) = &status.error {
            let soft = theme.soft(Tone::Error);
            body.push(
                Line::styled(
                    format!("  {} {error}", widgets::alert_icon(Tone::Error)),
                    Style::new().fg(soft.text),
                )
                .truncate(inner),
            );
        }
    }
    widgets::card(theme, Some(header), body, width)
}

fn cell(line: Line, width: usize, bg: Style) -> Line {
    if width == 0 {
        Line::new()
    } else {
        line.pad_to(width, bg)
    }
}

fn column_header(theme: &Theme, widths: &TableWidths, inner: usize) -> Line {
    let bg = Style::new().bg(theme.surface_2);
    let style = bg.fg(theme.text_tertiary).bold();
    let head = |name: &str, width: usize| cell(Line::styled(name, style), width, bg);
    Line::new()
        .space(MARKER_WIDTH, bg)
        .append(head("Package", widths.package))
        .append(head("Binary", widths.binary))
        .append(head("Installed", widths.installed))
        .append(head("Latest", widths.latest))
        .append(head(" Status", widths.status))
        .append(head(" Action", widths.action))
        .pad_to(inner, bg)
}

fn tool_row(
    theme: &Theme,
    widths: &TableWidths,
    status: &CrateStatus,
    selected: bool,
    inner: usize,
) -> Line {
    let row_bg = if selected {
        theme.surface_hover
    } else {
        theme.surface
    };
    let bg = Style::new().bg(row_bg);
    let marker = if selected {
        Line::styled("▌", bg.fg(theme.brand_solid)).space(1, bg)
    } else {
        Line::new().space(MARKER_WIDTH, bg)
    };

    let package = if selected {
        bg.fg(theme.text).bold()
    } else {
        bg.fg(theme.text)
    };
    let installed_style = if status.installed_version.is_some() {
        bg.fg(theme.text_secondary)
    } else {
        bg.fg(theme.text_disabled)
    };
    let latest_style = if status.latest_version.is_some() {
        bg.fg(theme.text_secondary)
    } else {
        bg.fg(theme.text_disabled)
    };

    let (tone, label) = status_kind(status);
    let badge = widgets::badge(theme, tone, label);
    let kind = match action_label(status) {
        "install" => ButtonKind::Primary,
        "update" => ButtonKind::Secondary,
        _ => ButtonKind::Disabled,
    };
    let button = widgets::button(theme, kind, action_text(status));

    marker
        .append(cell(
            Line::styled(&status.package, package),
            widths.package,
            bg,
        ))
        .append(cell(
            Line::styled(&status.binary, bg.fg(theme.text_tertiary)),
            widths.binary,
            bg,
        ))
        .append(cell(
            Line::styled(display_installed(status), installed_style),
            widths.installed,
            bg,
        ))
        .append(cell(
            Line::styled(display_latest(status), latest_style),
            widths.latest,
            bg,
        ))
        .append(cell(badge, widths.status, bg))
        .append(cell(button, widths.action, bg))
        .on(row_bg)
        .pad_to(inner, bg)
}
