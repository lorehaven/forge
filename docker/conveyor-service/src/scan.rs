//! Best-effort summaries of a repo's most recent code-quality steps.
//!
//! `anvil lint` (clippy), `anvil machete` (unused dependencies) and
//! `anvil audit` (known vulnerabilities) already exist as ordinary pipeline
//! steps - see `libs/conveyor-pipeline/src/steps/anvil.rs`. `cargo llvm-cov`
//! (test coverage) is a plain `run` step rather than an `anvil` one - there is
//! no tool of its own to wrap - so it is matched on its command text instead
//! of a step kind; see `CheckKind::from_command`. Nothing here triggers a run
//! or shells out to anything: it reads the most recent run's own steps and
//! parses whichever of the four it happened to execute. A repo whose
//! `.conveyor.toml` never runs one of these simply has nothing to show for it.
//!
//! Parsing is deliberately forgiving. `anvil`'s own output format is not a
//! contract conveyor owns, so every parser below falls back to "the step
//! passed" or "the step failed (exit N)" rather than guessing at a shape that
//! turned out to have changed.

use crate::domain::{Job, Run, Status};
use crate::scheduler::queue::{self, QueueError};
use quench_db::prelude::Db;

/// Every category this page knows how to summarise. Add a new one here and
/// in `run_kind` below to teach the page about another `anvil` step.
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
    /// The first word of an `anvil` step's command identifies `Lint`,
    /// `Machete` or `Audit`, e.g. `anvil lint --all-targets` regardless of the
    /// flags after it. `Coverage` is a plain `run` step - there is no `anvil
    /// coverage` - so it is matched on the command text containing
    /// `llvm-cov` instead, regardless of which of `cargo-llvm-cov`'s
    /// subcommands or flags produced this particular step.
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

/// One specific thing a check found - a clippy diagnostic, an unused
/// dependency, a RUSTSEC advisory. Fields are optional because the three
/// tools don't share a shape: `machete` has no severity or date, `lint` has
/// no advisory id, and so on. The overview cards only need `Vec<Finding>`'s
/// length; the detail subpage is what actually reads the rest of this.
#[derive(Debug, Clone, Default)]
pub struct Finding {
    pub title: String,
    /// `RUSTSEC-2023-0071`, for audit findings. Nothing else has one.
    pub id: Option<String>,
    /// `warning` / `error` for lint, the advisory's own CVSS-ish string
    /// (`5.9 (medium)`) or `unmaintained` / `yanked` for audit.
    pub severity: Option<String>,
    /// When the advisory was published, for audit - the closest thing to
    /// "when this was introduced" available without diffing dependency
    /// history across runs, which is its own, much bigger feature.
    pub date: Option<String>,
    /// `src/main.rs:10:9` for lint, the crate the dependency is unused in for
    /// machete.
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
    /// What the parser found, richest first for lint/audit's own ordering.
    /// Capped at 50 - a job that failed this badly needs its own log, not a
    /// summary page trying to hold all of it.
    pub findings: Vec<Finding>,
    /// What the overview card's big number should read, when the finding
    /// count itself isn't it. `None` for lint/machete/audit, where the count
    /// is the number worth leading with. `Some` for coverage: the count of
    /// incompletely-covered files hits the 50-finding cap on nearly any
    /// real codebase (that's the normal case, not a sign of trouble the way
    /// 50 live lint warnings would be), so it would read as an arbitrary,
    /// unexplained "50" instead of the coverage percentage the headline
    /// already states.
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

/// The most recent run for this repo, and whichever of the four checks its
/// jobs happened to run. `Ok(ScanSummary::default())` (not an error) when the
/// repo has never run, or has never run any of the four - both are "nothing
/// to show yet", not a failure.
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

    // Logs are appended once, when the job finishes (see
    // `queue::append_logs`), so a job with any finished step already has
    // everything below it settled - fetched once per job rather than once
    // per step.
    // `read_logs` is `seq > after`, so `0` would silently drop the very first
    // line (seq 0) - `-1` is the one value that means "from the start".
    let logs = queue::read_logs(db, &job.id, -1).await?;

    for (kind, step) in relevant {
        // A step still queued or running has nothing finished to parse yet -
        // its window has no end, and there is nothing wrong in leaving it out
        // until the run itself finishes.
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
            // A job may run several `llvm-cov` invocations (see .conveyor.toml's
            // `coverage` job) - regenerating a report from already-collected
            // profile data doesn't re-run anything, so whichever one parses is
            // kept, and a later one overwrites an earlier one rather than the
            // two being merged.
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

        // The tool's output did not look like anything this module knows how
        // to read - fall back to what the step itself already recorded.
        if self.passed {
            self.headline = "passed".to_string();
            return self;
        }

        self.headline = match exit_code {
            Some(code) => format!("failed (exit {code})"),
            None => "failed".to_string(),
        };
        // A failure with unparseable output has nothing else to summarise -
        // the tail of the log is the closest thing to a "finding" available,
        // so surface it. A passing step's tail (e.g. "10 modules scanned")
        // is not a finding and must not inflate the chip count above.
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

/// `cargo`'s own diagnostics: `warning: message` / `error: message` /
/// `warning[clippy::foo]: message`, each optionally followed by a `--> file:
/// line:col` location line. Cheap and format-stable enough for a summary -
/// this is not trying to be `--message-format=json`.
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
        // Nothing recognisable at all - most likely this ran clean and cargo
        // printed nothing but "Checking ..." lines, but it could also be an
        // unrecognised format. Say so rather than claiming a count of zero.
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

/// `cargo-machete`'s two shapes: a clean "didn't find any unused
/// dependencies" line, or one `crate -- path:` header per crate followed by
/// its indented, unused dependency names.
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

/// `cargo-audit`'s `Crate:`/`Title:`/`Date:`/`ID:`/`Severity:`/`Solution:`
/// blocks (vulnerabilities and `Warning: unmaintained`/`yanked` notices
/// share the same shape), separated by blank lines, plus its own final
/// "N vulnerabilities found" line when there is nothing to report.
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
            // `unmaintained` / `yanked` - a severity of sorts when there is no
            // CVSS-style one to show instead.
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

/// `cargo llvm-cov report`'s per-file table: a header row, a `---...` rule,
/// one row per file, then a trailing `TOTAL` row. Only positions 0
/// (filename), 7 (lines), 8 (missed lines) and 9 (line coverage %) are read -
/// `cargo-llvm-cov`'s column set (region/function/branch coverage too) has
/// changed release to release, but lines has stayed the seventh data column
/// through every version this has been checked against.
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
            // A fully-covered file is not a finding - nothing there needs a
            // reader's attention.
            files.push(row);
        }
    }

    let total = total?;

    // Worst first: the file with the most uncovered lines is the one most
    // worth opening, regardless of how large or small its percentage looks.
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
    // A rounded, shorter form for the overview card's big number - the
    // finding count itself would just be the (capped) count of files with any
    // gap at all, which is 50 on nearly every real codebase and says nothing
    // about how covered it actually is.
    let metric = format!("{:.0}%", total.line_pct);
    Some((headline, findings, metric))
}

/// One row of `cargo llvm-cov report`'s table - a file's, or the trailing
/// `TOTAL`'s.
struct CoverageRow {
    filename: String,
    lines: u64,
    missed_lines: u64,
    line_pct: f64,
}

impl CoverageRow {
    fn parse(line: &str) -> Option<Self> {
        let fields: Vec<&str> = line.split_whitespace().collect();
        // Filename, regions, missed regions, region%, functions, missed
        // functions, function% ("Executed" in the header), lines, missed
        // lines, line%, branches, missed branches, branch% - thirteen
        // columns when nothing is empty. `TOTAL` has no separate filename
        // column, but is otherwise the same shape.
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

/// Strips `ESC [ ... m` SGR sequences. `cargo`/`anvil` colour their output by
/// default, and a piped, non-tty subprocess does not always disable that -
/// leaving codes in place would break every `starts_with` check above.
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
