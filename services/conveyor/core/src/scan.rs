//! The code-quality checks of a repository: their order, labels and the scanner's severity levels.

use crate::model::{CheckChip, ScanFinding};

/// The checks in the order a page lists them: `(slug, title key, chip letter)`.
pub const CHECKS: [(&str, &str, &str); 6] = [
    ("lint", "ui_scan_lint_title", "L"),
    ("machete", "ui_scan_machete_title", "M"),
    ("audit", "ui_scan_audit_title", "A"),
    ("coverage", "ui_scan_coverage_title", "C"),
    ("tests", "ui_scan_tests_title", "T"),
    ("security", "ui_scan_security_title", "S"),
];

/// The i18n key of a check's title.
#[must_use]
pub fn title_key(kind: &str) -> Option<&'static str> {
    CHECKS.iter().find(|c| c.0 == kind).map(|c| c.1)
}

/// Only these keep a stored report page.
#[must_use]
pub fn has_report_page(kind: &str) -> bool {
    matches!(kind, "tests" | "security")
}

/// A chip as it is drawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chip {
    pub letter: &'static str,
    /// `chip-none`, `chip-clean`, `chip-warning` or `chip-danger`.
    pub class: &'static str,
    pub label: String,
    /// The i18n key of its tooltip.
    pub title_key: &'static str,
}

/// The chip of `kind` given what the latest scan said (`None`: it did not run).
#[must_use]
pub fn chip(kind: &str, check: Option<&CheckChip>) -> Chip {
    let (letter, title) = CHECKS
        .iter()
        .find(|c| c.0 == kind)
        .map_or(("?", "ui_chip_not_run"), |c| (c.2, c.1));
    match check {
        None => Chip {
            letter,
            class: "chip-none",
            label: "-".to_string(),
            title_key: "ui_chip_not_run",
        },
        Some(check) => {
            let count = check
                .metric
                .clone()
                .unwrap_or_else(|| check.findings.to_string());
            let class = if check.findings == 0 {
                "chip-clean"
            } else if check.passed {
                "chip-warning"
            } else {
                "chip-danger"
            };
            Chip {
                letter,
                class,
                label: count,
                title_key: title,
            }
        }
    }
}

/// Most severe first.
pub const LEVELS: [&str; 5] = ["critical", "high", "medium", "low", "info"];

/// How many findings each level has, in [`LEVELS`] order.
#[must_use]
pub fn counts(findings: &[ScanFinding]) -> [usize; 5] {
    let mut counts = [0; 5];
    for finding in findings {
        if let Some(at) = LEVELS.iter().position(|l| *l == finding.severity)
            && let Some(slot) = counts.get_mut(at)
        {
            *slot += 1;
        }
    }
    counts
}

/// The levels a page shows. Unticking everything is not a request for everything: a filter that was never applied shows every
/// level, one that was applied shows what is ticked.
#[must_use]
pub fn chosen_levels(ticked: &[&str], applied: bool) -> Vec<&'static str> {
    if !applied {
        return LEVELS.to_vec();
    }
    LEVELS
        .iter()
        .copied()
        .filter(|level| ticked.iter().any(|t| t.eq_ignore_ascii_case(level)))
        .collect()
}

/// The CSS class of a scanner severity.
#[must_use]
pub fn level_class(level: &str) -> String {
    format!("finding-severity finding-severity-{level}")
}

/// `unmaintained`/`yanked` read as warnings; other strings pass through as they are.
#[must_use]
pub fn severity_class(severity: &str) -> &'static str {
    match severity {
        "warning" | "unmaintained" | "yanked" => "finding-severity finding-severity-warning",
        "error" => "finding-severity finding-severity-error",
        _ => "finding-severity",
    }
}
