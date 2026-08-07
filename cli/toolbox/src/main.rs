use anyhow::{Context, Result};
use clap::Parser;
use crossterm::cursor::MoveTo;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::style::Stylize;
use crossterm::terminal::{
    Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use quench_cli::prelude::{Tone, print_status, require_binary};
use semver::Version;
use std::collections::HashMap;
use std::io::{Write, stdout};
use std::process::Command;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

const REGISTRY_NAME: &str = "ennor";
const REGISTRY_INDEX: &str = "sparse+https://ennor.ddns.net/index/";

#[derive(Clone, Copy)]
struct MonitoredCrate {
    package: &'static str,
    binary: &'static str,
}

/// Every installable the workspace builds, in the order they are listed.
///
/// `forge-toolbox` is deliberately absent: it cannot replace its own running
/// binary from inside this list, so it reports on itself through
/// [`toolbox_note`] instead. The services under `docker/` are absent too - they
/// ship as images, and nothing installs them with cargo.
const MONITORED_CRATES: &[MonitoredCrate] = &[
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

#[derive(Clone, Debug)]
struct CrateStatus {
    package: &'static str,
    binary: &'static str,
    installed_version: Option<Version>,
    latest_version: Option<Version>,
    installable: bool,
    updatable: bool,
    error: Option<String>,
}

#[derive(Parser)]
#[command(
    name = "forge-toolbox",
    version,
    about = "Interactive monitor and installer for Forge crates"
)]
struct Cli {}

struct App {
    selected: usize,
    statuses: Vec<CrateStatus>,
    toolbox_note: String,
    message: String,
}

struct InFlightAction {
    rx: Receiver<Result<String, String>>,
    selected: usize,
    spinner_idx: usize,
    label: String,
    last_tick: Instant,
}

#[derive(Clone)]
struct ActionRequest {
    package: String,
    installable: bool,
    installed: bool,
    updatable: bool,
}

#[derive(Clone, Copy)]
enum PlannedAction {
    Install,
    Update,
}

struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> Result<Self> {
        enable_raw_mode().context("failed to enable raw mode")?;
        execute!(stdout(), EnterAlternateScreen).context("failed to enter alternate screen")?;
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = execute!(stdout(), LeaveAlternateScreen);
        let _ = disable_raw_mode();
    }
}

fn main() -> Result<()> {
    let _ = Cli::parse();
    require_binary(
        "cargo",
        "toolbox drives cargo search/install against the workspace registry",
    )?;
    print_status(Tone::Info, "toolbox", "launching interactive monitor");
    run_repl()
}

fn run_repl() -> Result<()> {
    let mut app = refresh_app_state(0, "Ready")?;
    let _guard = TerminalGuard::enter()?;
    let mut dirty = true;
    let mut in_flight: Option<InFlightAction> = None;

    loop {
        if let Some(job) = in_flight.as_mut() {
            match job.rx.try_recv() {
                Ok(result) => {
                    let message = match result {
                        Ok(msg) => msg,
                        Err(err) => format!("action failed: {err}"),
                    };
                    let selected = job.selected;
                    in_flight = None;
                    app = refresh_app_state(selected, &message)?;
                    dirty = true;
                }
                Err(TryRecvError::Empty) => {
                    if job.last_tick.elapsed() >= Duration::from_millis(120) {
                        job.spinner_idx = (job.spinner_idx + 1) % spinner_frames().len();
                        job.last_tick = Instant::now();
                        dirty = true;
                    }
                }
                Err(TryRecvError::Disconnected) => {
                    let selected = job.selected;
                    in_flight = None;
                    app = refresh_app_state(selected, "action failed: worker disconnected")?;
                    dirty = true;
                }
            }
        }

        if dirty {
            let transient_status = in_flight
                .as_ref()
                .map(|job| format!("{} {}", spinner_frames()[job.spinner_idx], job.label));
            draw(&app, transient_status.as_deref())?;
            dirty = false;
        }

        if !event::poll(Duration::from_millis(80)).context("failed to poll terminal events")? {
            continue;
        }

        let evt = event::read().context("failed to read terminal event")?;
        let Event::Key(key) = evt else {
            continue;
        };

        if key.kind != KeyEventKind::Press {
            continue;
        }

        match key.code {
            KeyCode::Up => {
                app.selected = app.selected.saturating_sub(1);
                dirty = true;
            }
            KeyCode::Down if app.selected + 1 < app.statuses.len() => {
                app.selected += 1;
                dirty = true;
            }
            KeyCode::Char('q') => break,
            KeyCode::Char('r') => {
                if in_flight.is_some() {
                    continue;
                }
                app = refresh_app_state(app.selected, "Status refreshed")?;
                dirty = true;
            }
            KeyCode::Enter => {
                if in_flight.is_some() {
                    continue;
                }
                let selected = app.selected;
                let req = action_request(&app.statuses[selected]);
                let Some(planned) = planned_action(&req) else {
                    let message = run_selected_action(req)
                        .unwrap_or_else(|err| format!("action failed: {err}"));
                    app = refresh_app_state(selected, &message)?;
                    dirty = true;
                    continue;
                };
                let label = match planned {
                    PlannedAction::Install => format!("running {} (installing)", req.package),
                    PlannedAction::Update => format!("running {} (updating)", req.package),
                };
                let (tx, rx) = mpsc::channel::<Result<String, String>>();
                thread::spawn(move || {
                    let result = run_selected_action(req).map_err(|err| err.to_string());
                    let _ = tx.send(result);
                });
                in_flight = Some(InFlightAction {
                    rx,
                    selected,
                    spinner_idx: 0,
                    label,
                    last_tick: Instant::now(),
                });
                dirty = true;
            }
            _ => {}
        }
    }

    Ok(())
}

fn refresh_app_state(selected: usize, message: &str) -> Result<App> {
    let installed = installed_versions()?;
    let statuses = collect_statuses(&installed);
    let toolbox_note = toolbox_note(&installed);

    let safe_selected = if statuses.is_empty() {
        0
    } else {
        selected.min(statuses.len() - 1)
    };

    Ok(App {
        selected: safe_selected,
        statuses,
        toolbox_note,
        message: message.to_string(),
    })
}

fn draw(app: &App, transient_status: Option<&str>) -> Result<()> {
    let mut out = stdout();
    execute!(out, Clear(ClearType::All), MoveTo(0, 0)).context("failed to clear terminal")?;
    let (term_width, _) = crossterm::terminal::size().context("failed to read terminal size")?;
    let usable_width = (term_width as usize).saturating_sub(1);
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

    let mut line = |s: String| -> Result<()> {
        out.write_all(s.as_bytes())?;
        out.write_all(b"\r\n")?;
        Ok(())
    };

    line(format!("{}", "Forge Toolbox Status".bold().underlined()))?;
    line(String::new())?;
    line(format!(
        "{} {} {}",
        "registry:".dark_grey(),
        REGISTRY_NAME.cyan().bold(),
        format!("({REGISTRY_INDEX})").dark_grey()
    ))?;
    line(format!("{}", app.toolbox_note.as_str().yellow()))?;
    line(format!(
        "{} {}",
        "controls:".dark_grey(),
        "↑/↓ move, Enter install/update, r refresh, q quit".cyan()
    ))?;
    line(String::new())?;

    let top = make_border('┌', '┬', '┐', 2, &widths);
    let mid = make_border('├', '┼', '┤', 2, &widths);
    let bottom = make_border('└', '┴', '┘', 2, &widths);
    let top = fit_cell(&top, sep_len);
    let mid = fit_cell(&mid, sep_len);
    let bottom = fit_cell(&bottom, sep_len);

    line(format!("{}", top.dark_cyan()))?;
    line(format!(
        "│{}│{}│{}│{}│{}│{}│{}│",
        fit_cell("", 2).dark_cyan(),
        fit_cell("package", widths.package).bold().white(),
        fit_cell("binary", widths.binary).bold().white(),
        fit_cell("installed", widths.installed).bold().white(),
        fit_cell("latest", widths.latest).bold().white(),
        fit_cell("updatable", widths.updatable).bold().white(),
        fit_cell("action", widths.action).bold().white(),
    ))?;
    line(format!("{}", mid.dark_cyan()))?;

    for (idx, status) in app.statuses.iter().enumerate() {
        let marker = if idx == app.selected { ">" } else { " " };
        let installed = display_installed(status);
        let latest = display_latest(status);
        let updatable = display_updatable(status);
        let action = action_label(status);
        let marker = if idx == app.selected {
            fit_cell(marker, 2).green().bold().to_string()
        } else {
            fit_cell(marker, 2).dark_grey().to_string()
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

        line(format!(
            "│{}│{}│{}│{}│{}│{}│{}│",
            marker,
            fit_cell(status.package, widths.package).white(),
            fit_cell(status.binary, widths.binary).white(),
            installed_cell,
            latest_cell,
            updatable_cell,
            action_cell,
        ))?;

        if let Some(error) = &status.error {
            line(format!("{} {}", "warn:".red().bold(), error.as_str().red()))?;
        }
    }
    line(format!("{}", bottom.dark_cyan()))?;

    line(String::new())?;
    let status_text = transient_status.unwrap_or(app.message.as_str());
    line(format!("{} {}", "status:".dark_grey(), status_text.white()))?;

    out.flush().context("failed to flush terminal output")?;
    Ok(())
}

struct TableWidths {
    package: usize,
    binary: usize,
    installed: usize,
    latest: usize,
    updatable: usize,
    action: usize,
}

fn content_widths(statuses: &[CrateStatus]) -> TableWidths {
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

fn shrink_widths_to_fit(widths: &mut TableWidths, term_width: usize) {
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

fn make_border(
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

fn display_installed(status: &CrateStatus) -> String {
    match (&status.installed_version, status.installable) {
        (Some(v), _) => v.to_string(),
        (None, true) => "no (can install)".to_string(),
        (None, false) => "no".to_string(),
    }
}

fn display_latest(status: &CrateStatus) -> String {
    status
        .latest_version
        .as_ref()
        .map_or_else(|| "n/a".to_string(), |v| v.to_string())
}

fn display_updatable(status: &CrateStatus) -> &'static str {
    if status.updatable { "yes" } else { "no" }
}

fn char_width(value: &str) -> usize {
    value.chars().count()
}

fn fit_cell(value: &str, width: usize) -> String {
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

fn action_label(status: &CrateStatus) -> &'static str {
    if status.installed_version.is_none() && status.installable {
        "install"
    } else if status.updatable {
        "update"
    } else {
        "-"
    }
}

fn action_request(status: &CrateStatus) -> ActionRequest {
    ActionRequest {
        package: status.package.to_string(),
        installable: status.installable,
        installed: status.installed_version.is_some(),
        updatable: status.updatable,
    }
}

fn planned_action(req: &ActionRequest) -> Option<PlannedAction> {
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

fn run_selected_action(req: ActionRequest) -> Result<String> {
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

fn spinner_frames() -> &'static [&'static str] {
    &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]
}

fn collect_statuses(installed: &HashMap<String, Version>) -> Vec<CrateStatus> {
    MONITORED_CRATES
        .iter()
        .map(|item| {
            collect_status_for_package(item.package, item.binary, installed).unwrap_or_else(|err| {
                CrateStatus {
                    package: item.package,
                    binary: item.binary,
                    installed_version: installed.get(item.package).cloned(),
                    latest_version: None,
                    installable: false,
                    updatable: false,
                    error: Some(err.to_string()),
                }
            })
        })
        .collect()
}

fn collect_status_for_package(
    package: &'static str,
    binary: &'static str,
    installed: &HashMap<String, Version>,
) -> Result<CrateStatus> {
    let installed_version = installed.get(package).cloned();
    let latest_version = fetch_latest_registry_version(package)
        .with_context(|| format!("Failed to fetch latest version for {package}"))?;

    let installable = latest_version.is_some();
    let updatable = match (&installed_version, &latest_version) {
        (Some(current), Some(latest)) => latest > current,
        _ => false,
    };

    Ok(CrateStatus {
        package,
        binary,
        installed_version,
        latest_version,
        installable,
        updatable,
        error: None,
    })
}

fn toolbox_note(installed: &HashMap<String, Version>) -> String {
    let installed_toolbox = installed.get("forge-toolbox").cloned();
    let latest_toolbox = fetch_latest_registry_version("forge-toolbox")
        .ok()
        .flatten();

    match (installed_toolbox, latest_toolbox) {
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

fn fetch_latest_registry_version(package: &str) -> Result<Option<Version>> {
    let output = Command::new("cargo")
        .arg("search")
        .arg(package)
        .arg("--limit")
        .arg("10")
        .arg("--registry")
        .arg(REGISTRY_NAME)
        .output()
        .with_context(|| format!("Failed to execute cargo search for package {package}"))?;

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

fn parse_search_line(package: &str, line: &str) -> Result<Option<Version>> {
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

fn installed_versions() -> Result<HashMap<String, Version>> {
    let output = Command::new("cargo")
        .arg("install")
        .arg("--list")
        .output()
        .context("Failed to execute cargo install --list")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        anyhow::bail!("cargo install --list failed: {stderr}");
    }

    let stdout =
        String::from_utf8(output.stdout).context("Invalid UTF-8 from cargo install --list")?;
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

    Ok(versions)
}
