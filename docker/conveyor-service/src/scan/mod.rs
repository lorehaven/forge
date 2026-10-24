//! Best-effort summaries of a repo's most recent lint/machete/audit/coverage steps.
//!
//! Reads the last run's own step output; never triggers or shells out. The test results come from the
//! `summary.json` a `quench-test` run leaves among the job's artifacts in warehouse.

use crate::artifacts::WarehouseStore;
use crate::domain::{Job, Run, Status};
use crate::executors::Stream;
use crate::scheduler::queue::{self, QueueError};
use quench_db::prelude::Db;
use std::time::Duration;

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
    /// What the tests did, from the `summary.json` artifact (`quench-test`'s report).
    Tests,
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

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Lint => "ui_scan_lint_title",
            Self::Machete => "ui_scan_machete_title",
            Self::Audit => "ui_scan_audit_title",
            Self::Coverage => "ui_scan_coverage_title",
            Self::Tests => "ui_scan_tests_title",
        }
    }

    /// The URL segment this check's detail subpage lives at.
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Lint => "lint",
            Self::Machete => "machete",
            Self::Audit => "audit",
            Self::Coverage => "coverage",
            Self::Tests => "tests",
        }
    }

    #[must_use]
    pub fn from_slug(slug: &str) -> Option<Self> {
        match slug {
            "lint" => Some(Self::Lint),
            "machete" => Some(Self::Machete),
            "audit" => Some(Self::Audit),
            "coverage" => Some(Self::Coverage),
            "tests" => Some(Self::Tests),
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

#[derive(Debug, Clone)]
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
    /// Where conveyor can fetch the full report from warehouse, for the checks that keep one (tests).
    pub report_uri: Option<String>,
}

#[derive(Debug, Default)]
pub struct ScanSummary {
    pub run: Option<Run>,
    pub lint: Option<CheckResult>,
    pub machete: Option<CheckResult>,
    pub audit: Option<CheckResult>,
    pub coverage: Option<CheckResult>,
    pub tests: Option<CheckResult>,
}

impl ScanSummary {
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.lint.is_none()
            && self.machete.is_none()
            && self.audit.is_none()
            && self.coverage.is_none()
            && self.tests.is_none()
    }

    #[must_use]
    pub const fn get(&self, kind: CheckKind) -> Option<&CheckResult> {
        match kind {
            CheckKind::Lint => self.lint.as_ref(),
            CheckKind::Machete => self.machete.as_ref(),
            CheckKind::Audit => self.audit.as_ref(),
            CheckKind::Coverage => self.coverage.as_ref(),
            CheckKind::Tests => self.tests.as_ref(),
        }
    }
}

/// The most recent run and whichever checks its jobs happened to run.
/// `Ok(default())`, not an error, when there's nothing to show yet.
pub async fn latest(db: &Db, repo_id: &str) -> Result<ScanSummary, QueueError> {
    latest_with(db, repo_id, warehouse_store()).await
}

/// Warehouse as configured for this process (built once), or none when it keeps nothing.
#[must_use]
pub fn warehouse_store() -> Option<&'static WarehouseStore> {
    static STORE: std::sync::OnceLock<Option<WarehouseStore>> = std::sync::OnceLock::new();
    STORE.get_or_init(WarehouseStore::from_env).as_ref()
}

/// [`latest`] reading test summaries from `store` (none: no test summary).
pub async fn latest_with(
    db: &Db,
    repo_id: &str,
    store: Option<&WarehouseStore>,
) -> Result<ScanSummary, QueueError> {
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
        if let Some(store) = store
            && let Some(result) = job_tests(db, store, job).await
            // Several jobs may run tests (the coverage job runs them again): the one that saw most is the
            // summary.
            && summary.tests.as_ref().is_none_or(|kept| tests_seen(&result) > tests_seen(kept))
        {
            summary.tests = Some(result);
        }
    }

    Ok(summary)
}

/// How many tests a tests check saw: the total of its `passed/total` metric.
fn tests_seen(check: &CheckResult) -> u64 {
    check
        .metric
        .as_deref()
        .and_then(|metric| metric.split_once('/'))
        .and_then(|(_, total)| total.parse().ok())
        .unwrap_or(0)
}

/// Reports already fetched, by artifact id: the artifact never changes, and a page asks for it on every view.
type ReportCache = std::sync::Mutex<std::collections::HashMap<String, CheckResult>>;

fn report_cache() -> &'static ReportCache {
    static CACHE: std::sync::OnceLock<ReportCache> = std::sync::OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// The test summary a job left in warehouse, if it left one.
async fn job_tests(db: &Db, store: &WarehouseStore, job: &Job) -> Option<CheckResult> {
    let artifacts = queue::list_artifacts_for_job(db, &job.id).await.ok()?;
    let artifact = artifacts.iter().find(|a| a.name == TESTS_SUMMARY)?;

    let cached = report_cache()
        .lock()
        .ok()
        .and_then(|cache| cache.get(&artifact.id).cloned());
    let mut result = if let Some(result) = cached {
        result
    } else {
        let bytes = match store.download(&artifact.uri).await {
            Ok(bytes) => bytes,
            Err(error) => {
                quench_log::warn!("could not read the test summary of job {}: {error}", job.id);
                return None;
            }
        };
        let result = tests_check("", &String::from_utf8_lossy(&bytes))?;
        if let Ok(mut cache) = report_cache().lock() {
            if cache.len() >= 64 {
                cache.clear();
            }
            cache.insert(artifact.id.clone(), result.clone());
        }
        result
    };
    result.job_name = job.qualified_name();
    result.report_uri = artifacts
        .iter()
        .find(|a| a.name == TESTS_REPORT_PAGE)
        .map(|a| a.uri.clone());
    Some(result)
}

/// The file a `quench-test` run leaves with every test binary's results merged.
pub const TESTS_SUMMARY: &str = "summary.json";

/// The page that shows them, kept next to it.
pub const TESTS_REPORT_PAGE: &str = "index.html";

/// A `quench-test` report (`summary.json`) as a check: the totals as the headline, each failed test a finding.
#[must_use]
pub fn tests_check(job_name: &str, json: &str) -> Option<CheckResult> {
    use quench_serde::Value;
    let report: Value = quench_serde::json::from_str(json).ok()?;
    let totals = report.get("summary")?;
    let count = |key: &str| totals.get(key).and_then(Value::as_u64).unwrap_or(0);
    let (total, passed, failed, skipped) = (
        count("total"),
        count("passed"),
        count("failed"),
        count("skipped"),
    );

    let mut headline = format!("{passed} passed, {failed} failed, {skipped} skipped");
    if let Some(ms) = report.get("duration_ms").and_then(Value::as_u64)
        && ms > 0
    {
        headline = format!("{headline}, {}", duration_text(ms));
    }

    let mut findings = Vec::new();
    for suite in report.get("suites")?.as_array()? {
        let suite_name = suite
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let Some(cases) = suite.get("cases").and_then(Value::as_array) else {
            continue;
        };
        for case in cases {
            if case.get("status").and_then(Value::as_str) != Some("failed") {
                continue;
            }
            let name = case.get("name").and_then(Value::as_str).unwrap_or_default();
            findings.push(Finding {
                title: name.to_string(),
                severity: Some("failed".to_string()),
                location: case
                    .get("location")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .or_else(|| (!suite_name.is_empty()).then(|| suite_name.to_string())),
                extra: case
                    .get("message")
                    .and_then(Value::as_str)
                    .map(|message| message.trim().to_string())
                    .filter(|message| !message.is_empty()),
                ..Finding::default()
            });
        }
    }
    findings.truncate(MAX_FINDINGS);

    Some(CheckResult {
        kind: CheckKind::Tests,
        job_name: job_name.to_string(),
        passed: failed == 0,
        headline,
        findings,
        metric: Some(format!("{passed}/{total}")),
        report_uri: None,
    })
}

fn duration_text(ms: u64) -> String {
    if ms < 1000 {
        format!("{ms} ms")
    } else if ms < 60_000 {
        format!("{:.1} s", Duration::from_millis(ms).as_secs_f64())
    } else {
        format!("{}m {}s", ms / 60_000, (ms % 60_000) / 1000)
    }
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

        // Lint/machete/audit now read `anvil ... --json`: its structured
        // report is always on stdout, its human build/progress chatter
        // always on stderr (see `cli/anvil/src/util.rs::run_command_json`).
        // Keeping only stdout here is what makes joining possibly-many log
        // lines back into one JSON document safe - nothing from a
        // concurrently-written stderr line can land between them.
        let json_only = matches!(
            kind,
            CheckKind::Lint | CheckKind::Machete | CheckKind::Audit
        );
        let lines: Vec<&str> = logs
            .iter()
            .filter(|chunk| chunk.at >= start && chunk.at <= end)
            .filter(|chunk| !json_only || chunk.stream == Stream::Stdout)
            .map(|chunk| chunk.line.as_str())
            .collect();

        let passed = step.status == Status::Success;
        let result = CheckResult {
            kind,
            job_name: job.qualified_name(),
            headline: String::new(),
            findings: Vec::new(),
            metric: None,
            report_uri: None,
            passed,
        }
        .parsed(&lines, step.exit_code);

        match kind {
            CheckKind::Lint => summary.lint = Some(result),
            CheckKind::Machete => summary.machete = Some(result),
            CheckKind::Audit => summary.audit = Some(result),
            // A job may run several llvm-cov invocations; last one wins, not merged.
            CheckKind::Coverage => summary.coverage = Some(result),
            // Never matched from a step: the tests come from the job's artifacts (`job_tests`).
            CheckKind::Tests => {}
        }
    }

    Ok(())
}

/// Findings a summary page shows at all. Not a hard technical limit - just
/// where "a summary" stops and "you want the log" starts.
const MAX_FINDINGS: usize = 50;

impl CheckResult {
    #[must_use]
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
            CheckKind::Tests => None,
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

        self.headline = exit_code.map_or_else(
            || "failed".to_string(),
            |code| format!("failed (exit {code})"),
        );
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

/// Parses cargo and Clippy diagnostics from an Anvil lint step.
///
/// `anvil lint --json`: NDJSON, one `cargo`/clippy message per line (`cargo
/// clippy --message-format=json`'s own format - anvil passes it through
/// unmodified). Non-diagnostic lines (`compiler-artifact`, `build-finished`,
/// ...) are how a step with zero warnings is told apart from one whose
/// output isn't this format at all: seeing at least one recognised `reason`
/// is enough to trust "no warning/error lines" as a real "clean", not just
/// an unparsed log.
pub fn parse_lint(lines: &[&str]) -> Option<(String, Vec<Finding>)> {
    let mut warnings = 0usize;
    let mut errors = 0usize;
    let mut findings: Vec<Finding> = Vec::new();
    let mut saw_cargo_json = false;

    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(value) = quench_serde::json::from_str::<quench_serde::Value>(trimmed) else {
            continue;
        };
        if value
            .get("reason")
            .and_then(quench_serde::Value::as_str)
            .is_none()
        {
            continue;
        }
        saw_cargo_json = true;

        let Some(message) = value.get("message") else {
            continue;
        };
        let level = message
            .get("level")
            .and_then(quench_serde::Value::as_str)
            .unwrap_or_default();
        if level != "warning" && level != "error" {
            continue;
        }

        let title = message
            .get("message")
            .and_then(quench_serde::Value::as_str)
            .unwrap_or_default()
            .to_string();
        let location = message
            .get("spans")
            .and_then(quench_serde::Value::as_array)
            .and_then(|spans| {
                spans.iter().find(|span| {
                    span.get("is_primary")
                        .and_then(quench_serde::Value::as_bool)
                        == Some(true)
                })
            })
            .and_then(|span| {
                let file = span.get("file_name")?.as_str()?;
                let line = span.get("line_start")?.as_u64()?;
                let column = span.get("column_start")?.as_u64()?;
                Some(format!("{file}:{line}:{column}"))
            });

        if level == "warning" {
            warnings += 1;
        } else {
            errors += 1;
        }
        findings.push(Finding {
            title,
            severity: Some(level.to_string()),
            location,
            ..Finding::default()
        });
    }

    if !saw_cargo_json {
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

/// Parses Anvil's report of unused dependencies.
///
/// `anvil machete --json`: a single `{"findings": [{"package", "manifest",
/// "unused"}, ...]}` line (cargo-machete has no JSON of its own; this is
/// anvil's own report of its parsed text - see
/// `cli/anvil/src/commands/workspace.rs`). Always present, even when
/// `findings` is empty, so "no unused dependencies" and "nothing recognised
/// at all" stay distinguishable.
pub fn parse_machete(lines: &[&str]) -> Option<(String, Vec<Finding>)> {
    let joined = lines.join("\n");
    let value: quench_serde::Value = quench_serde::json::from_str(joined.trim()).ok()?;
    let entries = value.get("findings")?.as_array()?;

    let mut findings = Vec::new();
    for entry in entries {
        let Some(package) = entry.get("package").and_then(quench_serde::Value::as_str) else {
            continue;
        };
        let Some(unused) = entry.get("unused").and_then(quench_serde::Value::as_array) else {
            continue;
        };
        for dep in unused {
            let Some(dep) = dep.as_str() else { continue };
            findings.push(Finding {
                title: dep.to_string(),
                location: Some(package.to_string()),
                ..Finding::default()
            });
        }
    }

    if findings.is_empty() {
        return Some(("clean".to_string(), Vec::new()));
    }

    let headline = format!(
        "{} unused dependenc{}",
        findings.len(),
        plural_y(findings.len())
    );
    Some((headline, findings))
}

/// Parses vulnerability and advisory findings from an Anvil audit step.
///
/// `anvil audit --json`: a single `cargo-audit --format json` document (its
/// `vulnerabilities.list` plus every `warnings.*` entry - `unmaintained`,
/// `yanked`, ... - each shaped the same as a `list` entry). Always present,
/// so "no findings" and "nothing recognised" stay distinguishable the same
/// way as `parse_machete`.
pub fn parse_audit(lines: &[&str]) -> Option<(String, Vec<Finding>)> {
    let joined = lines.join("\n");
    let value: quench_serde::Value = quench_serde::json::from_str(joined.trim()).ok()?;

    let mut findings = Vec::new();
    if let Some(list) = value
        .get("vulnerabilities")
        .and_then(|v| v.get("list"))
        .and_then(quench_serde::Value::as_array)
    {
        findings.extend(list.iter().filter_map(audit_entry));
    }
    if let Some(warnings) = value
        .get("warnings")
        .and_then(quench_serde::Value::as_object)
    {
        for entries in warnings.values().filter_map(quench_serde::Value::as_array) {
            findings.extend(entries.iter().filter_map(audit_entry));
        }
    }

    if findings.is_empty() {
        return Some(("clean".to_string(), Vec::new()));
    }

    let headline = format!("{} finding{}", findings.len(), plural(findings.len()));
    Some((headline, findings))
}

/// One entry of `vulnerabilities.list` or a `warnings.*` list - both shaped
/// `{"advisory": {...}, "package": {"name", "version"}, "versions": {"patched": [...]}}`.
fn audit_entry(entry: &quench_serde::Value) -> Option<Finding> {
    let advisory = entry.get("advisory")?;
    let title = advisory
        .get("title")
        .and_then(quench_serde::Value::as_str)?
        .to_string();

    let severity = advisory
        .get("informational")
        .and_then(quench_serde::Value::as_str)
        .or_else(|| advisory.get("cvss").and_then(quench_serde::Value::as_str))
        .map(str::to_string);

    let package = entry.get("package");
    let krate = package
        .and_then(|p| p.get("name"))
        .and_then(quench_serde::Value::as_str);
    let version = package
        .and_then(|p| p.get("version"))
        .and_then(quench_serde::Value::as_str);
    let location = match (krate, version) {
        (Some(krate), Some(version)) => Some(format!("{krate} {version}")),
        (Some(krate), None) => Some(krate.to_string()),
        (None, _) => None,
    };

    let extra = entry
        .get("versions")
        .and_then(|v| v.get("patched"))
        .and_then(quench_serde::Value::as_array)
        .filter(|patched| !patched.is_empty())
        .map(|patched| {
            let versions = patched
                .iter()
                .filter_map(quench_serde::Value::as_str)
                .collect::<Vec<_>>()
                .join(", ");
            format!("Solution: Upgrade to {versions}")
        });

    Some(Finding {
        title,
        id: advisory
            .get("id")
            .and_then(quench_serde::Value::as_str)
            .map(str::to_string),
        severity,
        date: advisory
            .get("date")
            .and_then(quench_serde::Value::as_str)
            .map(str::to_string),
        location,
        extra,
    })
}

/// `cargo llvm-cov report`'s per-file table. Only columns 0/7/8/9 (filename,
/// lines, missed, line%) are read - the only ones stable across versions.
#[must_use]
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

const fn plural(count: usize) -> &'static str {
    if count == 1 { "" } else { "s" }
}

const fn plural_y(count: usize) -> &'static str {
    if count == 1 { "y" } else { "ies" }
}

/// Strips SGR color codes - a piped subprocess doesn't always disable them,
/// and left in place they'd break every `starts_with` check above.
#[must_use]
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
