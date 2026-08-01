//! Best-effort summaries of a repo's most recent code-quality steps.
//!
//! `anvil lint` (clippy), `anvil machete` (unused dependencies) and
//! `anvil audit` (known vulnerabilities) already exist as ordinary pipeline
//! steps - see `libs/conveyor-pipeline/src/steps/anvil.rs`. Nothing here
//! triggers a run or shells out to anything: it reads the most recent run's
//! own steps and parses whichever of the three it happened to execute. A
//! repo whose `.conveyor.toml` never runs one of these simply has nothing to
//! show for it.
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
    Lint,
    Machete,
    Audit,
}

impl CheckKind {
    /// The first word of an `anvil` step's command, e.g. `anvil lint
    /// --all-targets` is a `Lint` check regardless of the flags after it.
    fn from_command(command: &str) -> Option<Self> {
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
        }
    }

    /// The URL segment this check's detail subpage lives at.
    pub fn slug(self) -> &'static str {
        match self {
            Self::Lint => "lint",
            Self::Machete => "machete",
            Self::Audit => "audit",
        }
    }

    pub fn from_slug(slug: &str) -> Option<Self> {
        match slug {
            "lint" => Some(Self::Lint),
            "machete" => Some(Self::Machete),
            "audit" => Some(Self::Audit),
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
}

#[derive(Debug, Default)]
pub struct ScanSummary {
    pub run: Option<Run>,
    pub lint: Option<CheckResult>,
    pub machete: Option<CheckResult>,
    pub audit: Option<CheckResult>,
}

impl ScanSummary {
    pub fn is_empty(&self) -> bool {
        self.lint.is_none() && self.machete.is_none() && self.audit.is_none()
    }

    pub fn get(&self, kind: CheckKind) -> Option<&CheckResult> {
        match kind {
            CheckKind::Lint => self.lint.as_ref(),
            CheckKind::Machete => self.machete.as_ref(),
            CheckKind::Audit => self.audit.as_ref(),
        }
    }
}

/// The most recent run for this repo, and whichever of the three checks its
/// jobs happened to run. `Ok(ScanSummary::default())` (not an error) when the
/// repo has never run, or has never run any of the three - both are "nothing
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
            passed,
        }
        .parsed(&lines, step.exit_code);

        match kind {
            CheckKind::Lint => summary.lint = Some(result),
            CheckKind::Machete => summary.machete = Some(result),
            CheckKind::Audit => summary.audit = Some(result),
        }
    }

    Ok(())
}

/// Findings a summary page shows at all. Not a hard technical limit - just
/// where "a summary" stops and "you want the log" starts.
const MAX_FINDINGS: usize = 50;

impl CheckResult {
    fn parsed(mut self, lines: &[&str], exit_code: Option<i32>) -> Self {
        let stripped: Vec<String> = lines.iter().map(|line| strip_ansi(line)).collect();
        let borrowed: Vec<&str> = stripped.iter().map(String::as_str).collect();

        let parsed = match self.kind {
            CheckKind::Lint => parse_lint(&borrowed),
            CheckKind::Machete => parse_machete(&borrowed),
            CheckKind::Audit => parse_audit(&borrowed),
        };

        if let Some((headline, mut findings)) = parsed {
            findings.truncate(MAX_FINDINGS);
            self.headline = headline;
            self.findings = findings;
            return self;
        }

        // The tool's output did not look like anything this module knows how
        // to read - fall back to what the step itself already recorded.
        self.headline = if self.passed {
            "passed".to_string()
        } else {
            match exit_code {
                Some(code) => format!("failed (exit {code})"),
                None => "failed".to_string(),
            }
        };
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
fn parse_lint(lines: &[&str]) -> Option<(String, Vec<Finding>)> {
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
fn parse_machete(lines: &[&str]) -> Option<(String, Vec<Finding>)> {
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
fn parse_audit(lines: &[&str]) -> Option<(String, Vec<Finding>)> {
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

fn plural(count: usize) -> &'static str {
    if count == 1 { "" } else { "s" }
}

fn plural_y(count: usize) -> &'static str {
    if count == 1 { "y" } else { "ies" }
}

/// Strips `ESC [ ... m` SGR sequences. `cargo`/`anvil` colour their output by
/// default, and a piped, non-tty subprocess does not always disable that -
/// leaving codes in place would break every `starts_with` check above.
fn strip_ansi(line: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_sgr_sequences() {
        assert_eq!(
            strip_ansi("\u{1b}[33mwarning\u{1b}[0m: unused"),
            "warning: unused"
        );
        assert_eq!(strip_ansi("plain text"), "plain text");
    }

    #[test]
    fn parses_clean_lint() {
        assert!(parse_lint(&["Checking foo v0.1.0", "Finished"]).is_none());
    }

    #[test]
    fn parses_lint_warnings_with_location() {
        let lines = [
            "warning: unused variable: `x`",
            "  --> src/main.rs:10:9",
            "warning: unused import",
        ];
        let (headline, findings) = parse_lint(&lines).expect("should parse");
        assert_eq!(headline, "2 warnings");
        assert_eq!(findings.len(), 2);
        assert_eq!(findings[0].title, "unused variable: `x`");
        assert_eq!(findings[0].severity.as_deref(), Some("warning"));
        assert_eq!(findings[0].location.as_deref(), Some("src/main.rs:10:9"));
        assert_eq!(findings[1].location, None);
    }

    #[test]
    fn parses_bracketed_lint_diagnostics() {
        let lines = ["error[E0308]: mismatched types"];
        let (headline, findings) = parse_lint(&lines).expect("should parse");
        assert_eq!(headline, "1 error");
        assert_eq!(findings[0].title, "mismatched types");
        assert_eq!(findings[0].severity.as_deref(), Some("error"));
    }

    #[test]
    fn parses_clean_machete() {
        let lines =
            ["cargo-machete didn't find any unused dependencies in this directory. Good job!"];
        let (headline, findings) = parse_machete(&lines).expect("should parse");
        assert_eq!(headline, "clean");
        assert!(findings.is_empty());
    }

    #[test]
    fn parses_machete_findings() {
        let lines = ["foo -- ./crates/foo:", "    serde_yaml", "    once_cell"];
        let (headline, findings) = parse_machete(&lines).expect("should parse");
        assert_eq!(headline, "2 unused dependencies");
        assert_eq!(findings[0].title, "serde_yaml");
        assert_eq!(findings[0].location.as_deref(), Some("foo"));
        assert_eq!(findings[1].title, "once_cell");
    }

    #[test]
    fn parses_clean_audit() {
        let lines = ["Scanning Cargo.lock", "0 vulnerabilities found"];
        let (headline, findings) = parse_audit(&lines).expect("should parse");
        assert_eq!(headline, "clean");
        assert!(findings.is_empty());
    }

    #[test]
    fn parses_audit_findings_with_all_fields() {
        let lines = [
            "Crate:     time",
            "Version:   0.1.43",
            "Title:     Potential segfault",
            "Date:      2020-11-18",
            "ID:        RUSTSEC-2020-0071",
            "Severity:  6.2 (medium)",
            "Solution:  Upgrade to >=0.2.23",
        ];
        let (headline, findings) = parse_audit(&lines).expect("should parse");
        assert_eq!(headline, "1 finding");
        let finding = &findings[0];
        assert_eq!(finding.title, "Potential segfault");
        assert_eq!(finding.id.as_deref(), Some("RUSTSEC-2020-0071"));
        assert_eq!(finding.date.as_deref(), Some("2020-11-18"));
        assert_eq!(finding.severity.as_deref(), Some("6.2 (medium)"));
        assert_eq!(finding.location.as_deref(), Some("time 0.1.43"));
        assert_eq!(
            finding.extra.as_deref(),
            Some("Solution: Upgrade to >=0.2.23")
        );
    }

    #[test]
    fn parses_multiple_audit_blocks_separated_by_blank_lines() {
        let lines = [
            "Crate:     rsa",
            "Version:   0.9.10",
            "Title:     Marvin Attack",
            "Date:      2023-11-22",
            "ID:        RUSTSEC-2023-0071",
            "Severity:  5.9 (medium)",
            "Solution:  No fixed upgrade is available!",
            "",
            "Crate:     proc-macro-error2",
            "Version:   2.0.1",
            "Warning:   unmaintained",
            "Title:     proc-macro-error2 is unmaintained",
            "Date:      2026-06-07",
            "ID:        RUSTSEC-2026-0173",
        ];
        let (headline, findings) = parse_audit(&lines).expect("should parse");
        assert_eq!(headline, "2 findings");
        assert_eq!(findings[0].id.as_deref(), Some("RUSTSEC-2023-0071"));
        assert_eq!(findings[1].id.as_deref(), Some("RUSTSEC-2026-0173"));
        assert_eq!(findings[1].severity.as_deref(), Some("unmaintained"));
    }
}
