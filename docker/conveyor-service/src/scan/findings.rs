//! What `forge-sectest`'s live scan found: `security-findings.json`, kept by the nightly's job.
//!
//! The security card shows pass/fail of the gate's cases. This is the other half - every finding of
//! the scan at every severity, so low and medium ones can be reviewed without opening the file.

use quench_serde::Value;

/// The artifact the scan job declares.
pub const FINDINGS_ARTIFACT: &str = "security-findings.json";

/// Most severe first.
pub const LEVELS: [&str; 5] = ["critical", "high", "medium", "low", "info"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanFinding {
    pub severity: String,
    pub check: String,
    pub service: String,
    pub method: String,
    pub path: String,
    pub detail: String,
}

/// Position of `level` in [`LEVELS`]; unknown levels sort last.
#[must_use]
pub fn rank(level: &str) -> usize {
    LEVELS
        .iter()
        .position(|known| *known == level)
        .unwrap_or(LEVELS.len())
}

/// The findings of a report, most severe first, then by service, check and path. `None` when the text
/// is not a findings report.
#[must_use]
pub fn parse(json: &str) -> Option<Vec<ScanFinding>> {
    let report: Value = quench_serde::json::from_str(json).ok()?;
    let rows = report.as_array()?;
    let text = |row: &Value, key: &str| {
        row.get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let mut findings: Vec<ScanFinding> = rows
        .iter()
        .map(|row| ScanFinding {
            severity: text(row, "severity").to_ascii_lowercase(),
            check: text(row, "check"),
            service: text(row, "service"),
            method: text(row, "method"),
            path: text(row, "path"),
            detail: text(row, "detail"),
        })
        .collect();
    findings.sort_by(|a, b| {
        rank(&a.severity)
            .cmp(&rank(&b.severity))
            .then_with(|| a.service.cmp(&b.service))
            .then_with(|| a.check.cmp(&b.check))
            .then_with(|| a.path.cmp(&b.path))
            .then_with(|| a.detail.cmp(&b.detail))
    });
    Some(findings)
}

/// How many findings each level has, in [`LEVELS`] order.
#[must_use]
pub fn counts(findings: &[ScanFinding]) -> [usize; 5] {
    let mut counts = [0; 5];
    for finding in findings {
        if let Some(slot) = counts.get_mut(rank(&finding.severity)) {
            *slot += 1;
        }
    }
    counts
}

/// The levels a page asks for. Unticking everything is not a request for nothing: a page that has
/// not been filtered yet (`filtered` unset) shows every level.
#[must_use]
pub fn chosen_levels(requested: &[String], filtered: bool) -> Vec<&'static str> {
    if !filtered {
        return LEVELS.to_vec();
    }
    LEVELS
        .iter()
        .copied()
        .filter(|level| requested.iter().any(|r| r.eq_ignore_ascii_case(level)))
        .collect()
}
