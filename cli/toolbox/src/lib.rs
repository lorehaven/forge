use anyhow::{Context, Result};
use std::collections::HashMap;
use std::process::Command;

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

#[derive(Clone)]
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
