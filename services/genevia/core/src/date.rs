//! GEDCOM dates: the forms the data actually uses (`3 SEP 1993`, `SEP 1811`, `1765`, `ABT 1765`,
//! `BEF 1904`, `AFT 1900`), kept sortable and printable in Polish.

use std::fmt::Write as _;

const MONTHS: [&str; 12] = [
    "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC",
];
const PL: [&str; 12] = [
    "sty", "lut", "mar", "kwi", "maj", "cze", "lip", "sie", "wrz", "paź", "lis", "gru",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Qualifier {
    Exact,
    About,
    Before,
    After,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Date {
    pub qualifier: Qualifier,
    pub year: Option<i32>,
    pub month: Option<u8>,
    pub day: Option<u8>,
    /// What the file said, kept for dates that did not parse.
    pub raw: String,
}

impl Date {
    /// Parses a GEDCOM date. Anything unrecognised keeps only `raw`.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let raw = text.trim().to_string();
        let mut d = Self {
            qualifier: Qualifier::Exact,
            year: None,
            month: None,
            day: None,
            raw,
        };
        let upper = d.raw.to_uppercase();
        let mut words = upper.split_whitespace().peekable();
        match words.peek().copied() {
            Some("ABT" | "CAL" | "EST") => {
                d.qualifier = Qualifier::About;
                words.next();
            }
            Some("BEF") => {
                d.qualifier = Qualifier::Before;
                words.next();
            }
            Some("AFT") => {
                d.qualifier = Qualifier::After;
                words.next();
            }
            _ => {}
        }
        for w in words {
            if matches!(w, "FROM" | "TO" | "BET" | "AND") {
                // A period (`FROM 1925 TO 1960`): shown as written, translated, not boiled down to one year.
                d.year = None;
                d.month = None;
                d.day = None;
                return d;
            }
            if let Some(m) = MONTHS.iter().position(|m| *m == w) {
                d.month = Some(u8::try_from(m + 1).unwrap_or(1));
            } else if let Ok(n) = w.parse::<i32>() {
                if w.len() <= 2 && d.day.is_none() && d.month.is_none() {
                    d.day = u8::try_from(n).ok();
                } else {
                    d.year = Some(n);
                }
            }
        }
        d
    }

    /// A period or an odd date, in Polish where the words are known.
    fn period(&self) -> String {
        self.raw
            .split_whitespace()
            .map(|w| match w.to_uppercase().as_str() {
                "FROM" => "od".to_string(),
                "TO" => "do".to_string(),
                "BET" => "między".to_string(),
                "AND" => "a".to_string(),
                up => MONTHS
                    .iter()
                    .position(|m| *m == up)
                    .map_or_else(|| w.to_string(), |i| PL[i].to_string()),
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// A key that orders dates; unknown parts sort first.
    #[must_use]
    pub fn sort_key(&self) -> i64 {
        i64::from(self.year.unwrap_or(0)) * 10_000
            + i64::from(self.month.unwrap_or(0)) * 100
            + i64::from(self.day.unwrap_or(0))
    }

    /// The date as people write it in Poland: `3 wrz 1993`, `ok. 1765`, `przed 1904`.
    #[must_use]
    pub fn display(&self) -> String {
        let Some(year) = self.year else {
            return self.period();
        };
        let mut out = String::new();
        match self.qualifier {
            Qualifier::Exact => {}
            Qualifier::About => out.push_str("ok. "),
            Qualifier::Before => out.push_str("przed "),
            Qualifier::After => out.push_str("po "),
        }
        if let Some(day) = self.day {
            let _ = write!(out, "{day} ");
        }
        if let Some(m) = self.month {
            out.push_str(PL[usize::from(m - 1) % 12]);
            out.push(' ');
        }
        out.push_str(&year.to_string());
        out
    }
}

// ---------------------------------------------------------------------------------------------
// Dates as people type them (`21.08.1967`, `ok. 1900`, `od 1925 do 1960`) and as GEDCOM keeps them.
// ---------------------------------------------------------------------------------------------

fn part_from_input(p: &str) -> Option<String> {
    let nums: Vec<&str> = p.split('.').collect();
    let n = |s: &str, lo: u32, hi: u32| s.parse::<u32>().ok().filter(|v| (lo..=hi).contains(v));
    match nums.as_slice() {
        [y] if y.len() == 4 && y.chars().all(|c| c.is_ascii_digit()) => Some((*y).to_string()),
        [m, y] if y.len() == 4 => Some(format!(
            "{} {y}",
            MONTHS[usize::try_from(n(m, 1, 12)?).ok()? - 1]
        )),
        [d, m, y] if y.len() == 4 => Some(format!(
            "{} {} {y}",
            n(d, 1, 31)?,
            MONTHS[usize::try_from(n(m, 1, 12)?).ok()? - 1]
        )),
        _ => None,
    }
}

/// Turns what a person typed into GEDCOM's spelling: `21.08.1967` into `21 AUG 1967`, `ok. 1900`
/// into `ABT 1900`, `od 1925 do 1960` into `FROM 1925 TO 1960`. An empty text is an empty date.
///
/// # Errors
/// When the text is not a date this understands, with a sentence saying what is accepted.
pub fn from_input(text: &str) -> Result<String, String> {
    let bad = || {
        "Wpisz datę jako 21.08.1967, 08.1967 lub 1967, z dopiskiem ok., przed lub po, albo jako przedział (od … do …)."
            .to_string()
    };
    let t = text.trim();
    if t.is_empty() {
        return Ok(String::new());
    }
    let lower = t.to_lowercase();
    for (word, tag) in [
        ("ok.", "ABT"),
        ("ok", "ABT"),
        ("przed", "BEF"),
        ("po", "AFT"),
    ] {
        if let Some(rest) = lower.strip_prefix(word)
            && (rest.starts_with(' ') || word == "ok.")
        {
            return Ok(format!(
                "{tag} {}",
                part_from_input(rest.trim()).ok_or_else(bad)?
            ));
        }
    }
    if let Some(rest) = lower.strip_prefix("od ") {
        let (a, b) = rest.split_once(" do ").ok_or_else(bad)?;
        return Ok(format!(
            "FROM {} TO {}",
            part_from_input(a.trim()).ok_or_else(bad)?,
            part_from_input(b.trim()).ok_or_else(bad)?
        ));
    }
    if let Some(rest) = lower
        .strip_prefix("między ")
        .or_else(|| lower.strip_prefix("miedzy "))
    {
        let (a, b) = rest.split_once(" a ").ok_or_else(bad)?;
        return Ok(format!(
            "BET {} AND {}",
            part_from_input(a.trim()).ok_or_else(bad)?,
            part_from_input(b.trim()).ok_or_else(bad)?
        ));
    }
    // Already GEDCOM (`21 AUG 1967`, `ABT 1900`): keep it if it reads as a date.
    if Date::parse(t).year.is_some() && t.chars().any(|c| c.is_ascii_uppercase() || c == ' ') {
        return Ok(t.to_uppercase());
    }
    part_from_input(&lower).ok_or_else(bad)
}

fn part_to_input(p: &str) -> String {
    let d = Date::parse(p);
    match (d.day, d.month, d.year) {
        (Some(day), Some(m), Some(y)) => format!("{day:02}.{m:02}.{y}"),
        (None, Some(m), Some(y)) => format!("{m:02}.{y}"),
        (None, None, Some(y)) => y.to_string(),
        _ => p.to_string(),
    }
}

/// The reverse of [`from_input`]: GEDCOM's `21 AUG 1967` as `21.08.1967`, for a field to edit.
#[must_use]
pub fn to_input(raw: &str) -> String {
    let t = raw.trim();
    if let Some(rest) = t.strip_prefix("FROM ")
        && let Some((a, b)) = rest.split_once(" TO ")
    {
        return format!("od {} do {}", part_to_input(a), part_to_input(b));
    }
    if let Some(rest) = t.strip_prefix("BET ")
        && let Some((a, b)) = rest.split_once(" AND ")
    {
        return format!("między {} a {}", part_to_input(a), part_to_input(b));
    }
    for (tag, word) in [
        ("ABT ", "ok. "),
        ("CAL ", "ok. "),
        ("EST ", "ok. "),
        ("BEF ", "przed "),
        ("AFT ", "po "),
    ] {
        if let Some(rest) = t.strip_prefix(tag) {
            return format!("{word}{}", part_to_input(rest));
        }
    }
    if Date::parse(t).year.is_some() {
        part_to_input(t)
    } else {
        t.to_string()
    }
}
