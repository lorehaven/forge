//! Best-effort summaries of a repo's most recent lint/machete/audit/coverage
//! steps - reads the last run's own step output; never triggers or shells out.

use crate::domain::{Job, Run, Status};
use crate::scheduler::queue::{self, QueueError};
use quench_db::prelude::Db;

/// A category this page knows how to summarise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckKind {
    /// `anvil lint` (clippy).
    Lint,
    /// `anvil machete` (unused dependencies).
    Machete,
    /// `anvil audit` (known vulnerabilities).
    Audit,
    /// `cargo llvm-cov report` (test coverage).
    Coverage,
}

impl CheckKind {
    /// First word picks Lint/Machete/Audit; Coverage has no `anvil` command
    /// of its own, so it's matched on `llvm-cov` appearing anywhere instead.
    fn from_command(command: &str) -> Option<Self> {
        if command.contains("llvm-cov") {
            return Some(Self::Coverage);
        }
        match command.split_whitespace().next()? {
            "lint" => Some(Self::Lint),
            "machete" => Some(Self::Machete),
            "audit" => Some(Self::Audit),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Lint => "ui_scan_lint_title",
            Self::Machete => "ui_scan_machete_title",
            Self::Audit => "ui_scan_audit_title",
            Self::Coverage => "ui_scan_coverage_title",
        }
    }

    /// The URL segment this check's detail subpage lives at.
    pub fn slug(self) -> &'static str {
        match self {
            Self::Lint => "lint",
            Self::Machete => "machete",
            Self::Audit => "audit",
            Self::Coverage => "coverage",
        }
    }

    pub fn from_slug(slug: &str) -> Option<Self> {
        match slug {
            "lint" => Some(Self::Lint),
            "machete" => Some(Self::Machete),
            "audit" => Some(Self::Audit),
            "coverage" => Some(Self::Coverage),
            _ => None,
        }
    }
}

/// One thing a check found. Fields are optional since lint/machete/audit
/// don't share a shape - `severity`/`date`/`id` mean different things per tool.
#[derive(Debug, Clone, Default)]
pub struct Finding {
    pub title: String,
    /// `RUSTSEC-2023-0071`, for audit findings only.
    pub id: Option<String>,
    /// `warning`/`error` for lint, CVSS string or `unmaintained`/`yanked` for audit.
    pub severity: Option<String>,
    /// When the advisory was published, for audit.
    pub date: Option<String>,
    /// `src/main.rs:10:9` for lint, the unused-in crate for machete.
    pub location: Option<String>,
    /// Audit's `Solution:` line, when it has one.
    pub extra: Option<String>,
}

#[derive(Debug)]
pub struct CheckResult {
    pub kind: CheckKind,
    pub job_name: String,
    pub passed: bool,
    /// One line: "clean", "3 warnings", "1 vulnerability found", etc.
    pub headline: String,
    /// Capped at 50 - a job failed this badly needs its own log, not this page.
    pub findings: Vec<Finding>,
    /// The overview card's big number when the finding count isn't it - only
    /// `Some` for coverage, since 50 capped files says nothing about % covered.
    pub metric: Option<String>,
}

#[derive(Debug, Default)]
pub struct ScanSummary {
    pub run: Option<Run>,
    pub lint: Option<CheckResult>,
    pub machete: Option<CheckResult>,
    pub audit: Option<CheckResult>,
    pub coverage: Option<CheckResult>,
}

impl ScanSummary {
    pub fn is_empty(&self) -> bool {
        self.lint.is_none()
            && self.machete.is_none()
            && self.audit.is_none()
            && self.coverage.is_none()
    }

    pub fn get(&self, kind: CheckKind) -> Option<&CheckResult> {
        match kind {
            CheckKind::Lint => self.lint.as_ref(),
            CheckKind::Machete => self.machete.as_ref(),
            CheckKind::Audit => self.audit.as_ref(),
            CheckKind::Coverage => self.coverage.as_ref(),
        }
    }
}

/// The most recent run and whichever checks its jobs happened to run.
/// `Ok(default())`, not an error, when there's nothing to show yet.
pub async fn latest(db: &Db, repo_id: &str) -> Result<ScanSummary, QueueError> {
    let Some(run) = queue::list_runs(db, Some(repo_id), 1)
        .await?
        .into_iter()
        .next()
    else {
        return Ok(ScanSummary::default());
    };

    let jobs = queue::list_jobs(db, &run.id).await?;
    let mut summary = ScanSummary {
        run: Some(run),
        ..ScanSummary::default()
    };

    for job in &jobs {
        collect_job_checks(db, job, &mut summary).await?;
    }

    Ok(summary)
}

async fn collect_job_checks(
    db: &Db,
    job: &Job,
    summary: &mut ScanSummary,
) -> Result<(), QueueError> {
    let steps = queue::list_steps(db, &job.id).await?;
    let relevant: Vec<_> = steps
        .iter()
        .filter_map(|step| Some((CheckKind::from_command(&step.command)?, step)))
        .collect();

    if relevant.is_empty() {
        return Ok(());
    }

    // Fetched once per job, not per step - `-1` since `read_logs` is `seq > after` and `0` would drop line 0.
    let logs = queue::read_logs(db, &job.id, -1).await?;

    for (kind, step) in relevant {
        // Still queued or running: nothing finished to parse yet.
        let (Some(start), Some(end)) = (step.started_at, step.finished_at) else {
            continue;
        };

        let lines: Vec<&str> = logs
            .iter()
            .filter(|chunk| chunk.at >= start && chunk.at <= end)
            .map(|chunk| chunk.line.as_str())
            .collect();

        let passed = step.status == Status::Success;
        let result = CheckResult {
            kind,
            job_name: job.qualified_name(),
            headline: String::new(),
            findings: Vec::new(),
            metric: None,
            passed,
        }
        .parsed(&lines, step.exit_code);

        match kind {
            CheckKind::Lint => summary.lint = Some(result),
            CheckKind::Machete => summary.machete = Some(result),
            CheckKind::Audit => summary.audit = Some(result),
            // A job may run several llvm-cov invocations; last one wins, not merged.
            CheckKind::Coverage => summary.coverage = Some(result),
        }
    }

    Ok(())
}

/// Findings a summary page shows at all. Not a hard technical limit - just
/// where "a summary" stops and "you want the log" starts.
const MAX_FINDINGS: usize = 50;

impl CheckResult {
    pub fn parsed(mut self, lines: &[&str], exit_code: Option<i32>) -> Self {
        let stripped: Vec<String> = lines.iter().map(|line| strip_ansi(line)).collect();
        let borrowed: Vec<&str> = stripped.iter().map(String::as_str).collect();

        let parsed = match self.kind {
            CheckKind::Lint => {
                parse_lint(&borrowed).map(|(headline, findings)| (headline, findings, None))
            }
            CheckKind::Machete => {
                parse_machete(&borrowed).map(|(headline, findings)| (headline, findings, None))
            }
            CheckKind::Audit => {
                parse_audit(&borrowed).map(|(headline, findings)| (headline, findings, None))
            }
            CheckKind::Coverage => parse_coverage(&borrowed)
                .map(|(headline, findings, metric)| (headline, findings, Some(metric))),
        };

        if let Some((headline, mut findings, metric)) = parsed {
            findings.truncate(MAX_FINDINGS);
            self.headline = headline;
            self.findings = findings;
            self.metric = metric;
            return self;
        }

        // Unrecognised output - fall back to what the step itself recorded.
        if self.passed {
            self.headline = "passed".to_string();
            return self;
        }

        self.headline = match exit_code {
            Some(code) => format!("failed (exit {code})"),
            None => "failed".to_string(),
        };
        // Unparseable failure: the log tail is the closest thing to a finding.
        self.findings = stripped
            .iter()
            .rev()
            .take(10)
            .rev()
            .map(|line| line.trim_end().to_string())
            .filter(|line| !line.is_empty())
            .map(|title| Finding {
                title,
                ..Finding::default()
            })
            .collect();
        self
    }
}

/// `cargo`'s plain-text diagnostics, not `--message-format=json`.
pub fn parse_lint(lines: &[&str]) -> Option<(String, Vec<Finding>)> {
    let mut warnings = 0usize;
    let mut errors = 0usize;
    let mut findings: Vec<Finding> = Vec::new();

    for line in lines {
        let trimmed = line.trim();

        let bracketed = |prefix: &str| trimmed.split_once(prefix).map(|(_, rest)| rest.trim());
        let (kind, title) = if let Some(rest) = trimmed.strip_prefix("warning: ") {
            ("warning", rest.trim().to_string())
        } else if let Some(rest) = trimmed.strip_prefix("error: ") {
            ("error", rest.trim().to_string())
        } else if trimmed.starts_with("warning[") {
            ("warning", bracketed("]: ").unwrap_or(trimmed).to_string())
        } else if trimmed.starts_with("error[") {
            ("error", bracketed("]: ").unwrap_or(trimmed).to_string())
        } else if let Some(location) = trimmed.strip_prefix("-->") {
            if let Some(finding) = findings.last_mut() {
                finding.location = Some(location.trim().to_string());
            }
            continue;
        } else {
            continue;
        };

        if kind == "warning" {
            warnings += 1;
        } else {
            errors += 1;
        }
        findings.push(Finding {
            title,
            severity: Some(kind.to_string()),
            ..Finding::default()
        });
    }

    if warnings == 0 && errors == 0 {
        // Say so rather than claiming zero - could be clean, could be unrecognised.
        return None;
    }

    let headline = match (warnings, errors) {
        (0, 0) => "clean".to_string(),
        (w, 0) => format!("{w} warning{}", plural(w)),
        (0, e) => format!("{e} error{}", plural(e)),
        (w, e) => format!("{w} warning{}, {e} error{}", plural(w), plural(e)),
    };
    Some((headline, findings))
}

/// `cargo-machete`'s two shapes: a clean line, or one `crate -- path:` header
/// per crate followed by its indented, unused dependency names.
pub fn parse_machete(lines: &[&str]) -> Option<(String, Vec<Finding>)> {
    if lines
        .iter()
        .any(|line| line.contains("didn't find any unused dependencies"))
    {
        return Some(("clean".to_string(), Vec::new()));
    }

    let mut current_crate: Option<&str> = None;
    let mut findings = Vec::new();

    for line in lines {
        if let Some(header) = line.strip_suffix(':').filter(|_| line.contains(" -- ")) {
            current_crate = header.split(" -- ").next();
            continue;
        }
        if line.starts_with(char::is_whitespace) && !line.trim().is_empty() {
            findings.push(Finding {
                title: line.trim().to_string(),
                location: current_crate.map(str::to_string),
                ..Finding::default()
            });
        }
    }

    if findings.is_empty() {
        return None;
    }

    let headline = format!(
        "{} unused dependenc{}",
        findings.len(),
        plural_y(findings.len())
    );
    Some((headline, findings))
}

/// `cargo-audit`'s `Crate:`/`Title:`/... blocks, blank-line separated, plus
/// its "N vulnerabilities found" line when there's nothing to report.
pub fn parse_audit(lines: &[&str]) -> Option<(String, Vec<Finding>)> {
    let mut findings = Vec::new();
    let mut block: Vec<&str> = Vec::new();

    for line in lines {
        if line.trim().is_empty() {
            if let Some(finding) = audit_block(&block) {
                findings.push(finding);
            }
            block.clear();
        } else {
            block.push(line);
        }
    }
    if let Some(finding) = audit_block(&block) {
        findings.push(finding);
    }

    if findings.is_empty() {
        if lines
            .iter()
            .any(|line| line.contains("0 vulnerabilities found"))
        {
            return Some(("clean".to_string(), Vec::new()));
        }
        return None;
    }

    let headline = format!("{} finding{}", findings.len(), plural(findings.len()));
    Some((headline, findings))
}

fn audit_block(block: &[&str]) -> Option<Finding> {
    let mut finding = Finding::default();
    let mut krate = None;
    let mut version = None;

    for line in block {
        if let Some(v) = line.strip_prefix("Crate:") {
            krate = Some(v.trim().to_string());
        } else if let Some(v) = line.strip_prefix("Version:") {
            version = Some(v.trim().to_string());
        } else if let Some(v) = line.strip_prefix("Title:") {
            finding.title = v.trim().to_string();
        } else if let Some(v) = line.strip_prefix("Date:") {
            finding.date = Some(v.trim().to_string());
        } else if let Some(v) = line.strip_prefix("ID:") {
            finding.id = Some(v.trim().to_string());
        } else if let Some(v) = line.strip_prefix("Severity:") {
            finding.severity = Some(v.trim().to_string());
        } else if let Some(v) = line.strip_prefix("Solution:") {
            finding.extra = Some(format!("Solution: {}", v.trim()));
        } else if let Some(v) = line.strip_prefix("Warning:") {
            // `unmaintained`/`yanked` - a severity of sorts when there's no CVSS one.
            finding.severity.get_or_insert_with(|| v.trim().to_string());
        }
    }

    if finding.title.is_empty() {
        return None;
    }
    finding.location = match (krate, version) {
        (Some(krate), Some(version)) => Some(format!("{krate} {version}")),
        (Some(krate), None) => Some(krate),
        (None, _) => None,
    };
    Some(finding)
}

/// `cargo llvm-cov report`'s per-file table. Only columns 0/7/8/9 (filename,
/// lines, missed, line%) are read - the only ones stable across versions.
pub fn parse_coverage(lines: &[&str]) -> Option<(String, Vec<Finding>, String)> {
    let mut files = Vec::new();
    let mut total: Option<CoverageRow> = None;

    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("Filename") || trimmed.starts_with('-') {
            continue;
        }

        let Some(row) = CoverageRow::parse(trimmed) else {
            continue;
        };

        if row.filename == "TOTAL" {
            total = Some(row);
        } else if row.missed_lines > 0 {
            // A fully-covered file is not a finding.
            files.push(row);
        }
    }

    let total = total?;

    // Worst first: most uncovered lines, not lowest percentage.
    files.sort_by_key(|row| std::cmp::Reverse(row.missed_lines));

    let findings = files
        .into_iter()
        .map(|row| Finding {
            title: row.filename,
            severity: Some(format!("{:.2}%", row.line_pct)),
            location: Some(format!(
                "{} of {} lines missed",
                row.missed_lines, row.lines
            )),
            ..Finding::default()
        })
        .collect();

    let headline = format!("{:.2}% line coverage", total.line_pct);
    // Rounded, for the overview card - the capped finding count says nothing about % covered.
    let metric = format!("{:.0}%", total.line_pct);
    Some((headline, findings, metric))
}

/// One row of the table - a file's, or the trailing `TOTAL`'s.
struct CoverageRow {
    filename: String,
    lines: u64,
    missed_lines: u64,
    line_pct: f64,
}

impl CoverageRow {
    fn parse(line: &str) -> Option<Self> {
        let fields: Vec<&str> = line.split_whitespace().collect();
        // 13 columns when nothing's empty; `TOTAL` is otherwise the same shape.
        if fields.len() < 10 {
            return None;
        }

        Some(Self {
            filename: fields[0].to_string(),
            lines: fields[7].parse().ok()?,
            missed_lines: fields[8].parse().ok()?,
            line_pct: fields[9].strip_suffix('%')?.parse().ok()?,
        })
    }
}

fn plural(count: usize) -> &'static str {
    if count == 1 { "" } else { "s" }
}

fn plural_y(count: usize) -> &'static str {
    if count == 1 { "y" } else { "ies" }
}

/// Strips SGR color codes - a piped subprocess doesn't always disable them,
/// and left in place they'd break every `starts_with` check above.
pub fn strip_ansi(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            for next in chars.by_ref() {
                if next.is_ascii_alphabetic() {
                    break;
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}
