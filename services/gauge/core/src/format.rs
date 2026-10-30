//! Numbers as people read them.

#[must_use]
pub fn bytes(value: f64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut v = value.max(0.0);
    let mut unit = 0;
    while v >= 1024.0 && unit < UNITS.len() - 1 {
        v /= 1024.0;
        unit += 1;
    }
    if unit == 0 || v >= 100.0 {
        format!("{v:.0} {}", UNITS[unit])
    } else if v >= 10.0 {
        format!("{v:.1} {}", UNITS[unit])
    } else {
        format!("{v:.2} {}", UNITS[unit])
    }
}

#[must_use]
pub fn rate(value: f64) -> String {
    format!("{}/s", bytes(value))
}

#[must_use]
pub fn pct(value: f64) -> String {
    format!("{value:.0}%")
}

#[must_use]
pub fn duration(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    let (d, h, m) = (s / 86_400, s % 86_400 / 3600, s % 3600 / 60);
    if d > 0 {
        format!("{d}d {h}h")
    } else if h > 0 {
        format!("{h}h {m}m")
    } else {
        format!("{m}m")
    }
}

/// A span as an axis label: `45 min`, `6 h`, `1.5 d`.
#[must_use]
pub fn span(secs: f64) -> String {
    let trim = |v: f64| {
        let text = format!("{v:.1}");
        text.strip_suffix(".0").unwrap_or(&text).to_string()
    };
    if secs >= 86_400.0 {
        format!("{} d", trim(secs / 86_400.0))
    } else if secs >= 3600.0 {
        format!("{} h", trim(secs / 3600.0))
    } else {
        format!("{} min", trim(secs / 60.0))
    }
}

/// "3h 12m ago" for a time `secs` in the past; "just now" under a minute.
#[must_use]
pub fn ago(secs: f64) -> String {
    if secs < 60.0 {
        "just now".to_string()
    } else {
        format!("{} ago", duration(secs))
    }
}

/// How full a bar is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Ok,
    Warn,
    Crit,
}

impl Level {
    /// Under 75 % is fine, from 75 % a warning, from 90 % critical.
    #[must_use]
    pub fn of(pct: f64) -> Self {
        if pct >= 90.0 {
            Self::Crit
        } else if pct >= 75.0 {
            Self::Warn
        } else {
            Self::Ok
        }
    }

    /// The class that colours a bar.
    #[must_use]
    pub const fn class(self) -> &'static str {
        match self {
            Self::Ok => "gg-ok",
            Self::Warn => "gg-warn",
            Self::Crit => "gg-crit",
        }
    }
}
