//! What a check reports.

use std::fmt::{self, Write};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

impl Severity {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }

    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        [Self::Info, Self::Low, Self::Medium, Self::High, Self::Critical]
            .into_iter()
            .find(|s| s.as_str().eq_ignore_ascii_case(text))
    }
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub severity: Severity,
    pub check: &'static str,
    pub service: String,
    pub method: String,
    pub path: String,
    pub detail: String,
}

impl Finding {
    #[must_use]
    pub fn new(
        severity: Severity,
        check: &'static str,
        service: &str,
        method: &str,
        path: &str,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            severity,
            check,
            service: service.to_string(),
            method: method.to_string(),
            path: path.to_string(),
            detail: detail.into(),
        }
    }

    #[must_use]
    pub fn to_json(&self) -> String {
        format!(
            r#"{{"severity":"{}","check":"{}","service":"{}","method":"{}","path":"{}","detail":"{}"}}"#,
            self.severity,
            self.check,
            escape(&self.service),
            escape(&self.method),
            escape(&self.path),
            escape(&self.detail)
        )
    }
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{:<8}] {:<10} {} {} {}\n           {}",
            self.severity.as_str().to_uppercase(),
            self.check,
            self.service,
            self.method,
            self.path,
            self.detail
        )
    }
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
}

/// A JSON array of every finding.
#[must_use]
pub fn report_json(findings: &[Finding]) -> String {
    let items: Vec<String> = findings.iter().map(Finding::to_json).collect();
    format!("[{}]", items.join(",\n "))
}
