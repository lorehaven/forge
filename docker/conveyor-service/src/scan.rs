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
}

#[derive(Debug)]
pub struct CheckResult {
    pub kind: CheckKind,
    pub job_name: String,
    pub passed: bool,
    /// One line: "clean", "3 warnings", "1 vulnerability found", etc.
    pub headline: String,
    /// A handful of the most relevant lines - specific findings when parsing
    /// found any, otherwise the tail of the raw log.
    pub details: Vec<String>,
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
            details: Vec::new(),
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

impl CheckResult {
    fn parsed(mut self, lines: &[&str], exit_code: Option<i32>) -> Self {
        let stripped: Vec<String> = lines.iter().map(|line| strip_ansi(line)).collect();
        let borrowed: Vec<&str> = stripped.iter().map(String::as_str).collect();

        let parsed = match self.kind {
            CheckKind::Lint => parse_lint(&borrowed),
            CheckKind::Machete => parse_machete(&borrowed),
            CheckKind::Audit => parse_audit(&borrowed),
        };

        if let Some((headline, details)) = parsed {
            self.headline = headline;
            self.details = details;
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
        self.details = stripped
            .iter()
            .rev()
            .take(10)
            .rev()
            .map(|line| line.trim_end().to_string())
            .filter(|line| !line.is_empty())
            .collect();
        self
    }
}

/// `cargo`'s own diagnostics, one per line: `warning: ...` / `error: ...` /
/// `error[E0000]: ...`. Cheap and format-stable enough for a summary count -
/// this is not trying to be `--message-format=json`.
fn parse_lint(lines: &[&str]) -> Option<(String, Vec<String>)> {
    let mut warnings = 0usize;
    let mut errors = 0usize;
    let mut details = Vec::new();

    for line in lines {
        let trimmed = line.trim_start();
        if trimmed.starts_with("warning:") || trimmed.starts_with("warning[") {
            warnings += 1;
            details.push(trimmed.to_string());
        } else if trimmed.starts_with("error:") || trimmed.starts_with("error[") {
            errors += 1;
            details.push(trimmed.to_string());
        }
    }

    if warnings == 0 && errors == 0 {
        // Nothing recognisable at all - most likely this ran clean and cargo
        // printed nothing but "Checking ..." lines, but it could also be an
        // unrecognised format. Say so rather than claiming a count of zero.
        return None;
    }

    details.truncate(20);
    let headline = match (warnings, errors) {
        (0, 0) => "clean".to_string(),
        (w, 0) => format!("{w} warning{}", plural(w)),
        (0, e) => format!("{e} error{}", plural(e)),
        (w, e) => format!("{w} warning{}, {e} error{}", plural(w), plural(e)),
    };
    Some((headline, details))
}

/// `cargo-machete`'s two shapes: a clean "didn't find any unused
/// dependencies" line, or one `crate -- path:` header per crate followed by
/// its indented, unused dependency names.
fn parse_machete(lines: &[&str]) -> Option<(String, Vec<String>)> {
    if lines
        .iter()
        .any(|line| line.contains("didn't find any unused dependencies"))
    {
        return Some(("clean".to_string(), Vec::new()));
    }

    let mut current_crate: Option<&str> = None;
    let mut details = Vec::new();

    for line in lines {
        if let Some(header) = line.strip_suffix(':').filter(|_| line.contains(" -- ")) {
            current_crate = header.split(" -- ").next();
            continue;
        }
        if line.starts_with(char::is_whitespace) && !line.trim().is_empty() {
            let dep = line.trim();
            match current_crate {
                Some(krate) => details.push(format!("{krate}: {dep}")),
                None => details.push(dep.to_string()),
            }
        }
    }

    if details.is_empty() {
        return None;
    }

    let headline = format!(
        "{} unused dependenc{}",
        details.len(),
        plural_y(details.len())
    );
    details.truncate(20);
    Some((headline, details))
}

/// `cargo-audit`'s `ID:`/`Title:`/`Severity:` blocks, one per advisory, plus
/// its own final "N vulnerabilities found" line when present.
fn parse_audit(lines: &[&str]) -> Option<(String, Vec<String>)> {
    let mut details = Vec::new();
    // `Title:` precedes `ID:` within one advisory block, so a block is
    // complete - and pushed - the moment its `ID:` line arrives.
    let mut current_title: Option<String> = None;

    for line in lines {
        if let Some(title) = line.strip_prefix("Title:") {
            current_title = Some(title.trim().to_string());
        } else if let Some(id) = line.strip_prefix("ID:")
            && let Some(title) = current_title.take()
        {
            details.push(format!("{}: {title}", id.trim()));
        }
    }

    if details.is_empty() {
        if lines
            .iter()
            .any(|line| line.contains("0 vulnerabilities found"))
        {
            return Some(("clean".to_string(), Vec::new()));
        }
        return None;
    }

    let headline = format!(
        "{} vulnerabilit{} found",
        details.len(),
        plural_y(details.len())
    );
    details.truncate(20);
    Some((headline, details))
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
        assert_eq!(parse_lint(&["Checking foo v0.1.0", "Finished"]), None);
    }

    #[test]
    fn parses_lint_warnings() {
        let lines = ["warning: unused variable: `x`", "warning: unused import"];
        let (headline, details) = parse_lint(&lines).expect("should parse");
        assert_eq!(headline, "2 warnings");
        assert_eq!(details.len(), 2);
    }

    #[test]
    fn parses_clean_machete() {
        let lines =
            ["cargo-machete didn't find any unused dependencies in this directory. Good job!"];
        let (headline, details) = parse_machete(&lines).expect("should parse");
        assert_eq!(headline, "clean");
        assert!(details.is_empty());
    }

    #[test]
    fn parses_machete_findings() {
        let lines = ["foo -- ./crates/foo:", "    serde_yaml", "    once_cell"];
        let (headline, details) = parse_machete(&lines).expect("should parse");
        assert_eq!(headline, "2 unused dependencies");
        assert_eq!(details, vec!["foo: serde_yaml", "foo: once_cell"]);
    }

    #[test]
    fn parses_clean_audit() {
        let lines = ["Scanning Cargo.lock", "0 vulnerabilities found"];
        let (headline, details) = parse_audit(&lines).expect("should parse");
        assert_eq!(headline, "clean");
        assert!(details.is_empty());
    }

    #[test]
    fn parses_audit_findings() {
        let lines = [
            "Crate:     time",
            "Title:     Potential segfault",
            "ID:        RUSTSEC-2020-0071",
        ];
        let (headline, details) = parse_audit(&lines).expect("should parse");
        assert_eq!(headline, "1 vulnerability found");
        assert_eq!(details, vec!["RUSTSEC-2020-0071: Potential segfault"]);
    }
}
