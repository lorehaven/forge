//! A reader for the Prometheus text exposition format.
//!
//! Both `node_exporter` and `pmproxy`'s `/metrics` speak it. PCP also puts each metric's units in a `# PCP5 name pmid type indom sem units`
//! comment; those are kept so values can be scaled without a table of metric names.

use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub struct Sample {
    pub name: String,
    pub labels: Vec<(String, String)>,
    pub value: f64,
}

impl Sample {
    #[must_use]
    pub fn label(&self, key: &str) -> Option<&str> {
        self.labels
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
}

#[derive(Debug, Default)]
pub struct Exposition {
    pub samples: Vec<Sample>,
    /// PCP metric name (with underscores, as it appears in samples) -> its units text (`Kbyte`, `millisec`).
    pub units: HashMap<String, String>,
}

impl Exposition {
    pub fn named<'a>(&'a self, name: &str) -> impl Iterator<Item = &'a Sample> + use<'a> {
        let name = name.to_string();
        self.samples.iter().filter(move |s| s.name == name)
    }

    /// The first sample of `name`, if any.
    #[must_use]
    pub fn one(&self, name: &str) -> Option<f64> {
        self.named(name).next().map(|s| s.value)
    }

    /// The factor that turns a value in the metric's PCP space units into bytes (1 when unitless or unknown).
    #[must_use]
    pub fn byte_scale(&self, name: &str) -> f64 {
        match self.units.get(name).map(String::as_str) {
            Some("Kbyte") => 1024.0,
            Some("Mbyte") => 1024.0 * 1024.0,
            Some("Gbyte") => 1024.0 * 1024.0 * 1024.0,
            Some("Tbyte") => 1024.0_f64.powi(4),
            _ => 1.0,
        }
    }

    /// The factor that turns a value in the metric's PCP time units into seconds.
    #[must_use]
    pub fn second_scale(&self, name: &str) -> f64 {
        match self.units.get(name).map(String::as_str) {
            Some("nanosec") => 1e-9,
            Some("microsec") => 1e-6,
            Some("millisec") => 1e-3,
            Some("minute") => 60.0,
            Some("hour") => 3600.0,
            _ => 1.0,
        }
    }
}

#[must_use]
pub fn parse(text: &str) -> Exposition {
    let mut out = Exposition::default();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("# PCP5 ") {
            // name pmid type indom sem [units...]
            let fields: Vec<&str> = rest.split_whitespace().collect();
            if fields.len() >= 6 {
                out.units
                    .insert(fields[0].replace('.', "_"), fields[5..].join(" "));
            }
            continue;
        }
        if line.starts_with('#') {
            continue;
        }
        if let Some(sample) = parse_sample(line) {
            out.samples.push(sample);
        }
    }
    out
}

fn parse_sample(line: &str) -> Option<Sample> {
    let name_end = line.find(['{', ' ', '\t']).unwrap_or(line.len());
    let name = line[..name_end].to_string();
    let mut rest = &line[name_end..];
    let mut labels = Vec::new();
    if rest.starts_with('{') {
        let (parsed, after) = parse_labels(&rest[1..])?;
        labels = parsed;
        rest = after;
    }
    let value = parse_value(rest.split_whitespace().next()?)?;
    Some(Sample {
        name,
        labels,
        value,
    })
}

fn parse_value(text: &str) -> Option<f64> {
    match text {
        "NaN" => Some(f64::NAN),
        "+Inf" | "Inf" => Some(f64::INFINITY),
        "-Inf" => Some(f64::NEG_INFINITY),
        _ => text.parse().ok(),
    }
}

/// Reads `k="v",k2="v2"}` and returns the labels and what follows the closing brace.
fn parse_labels(mut s: &str) -> Option<(Vec<(String, String)>, &str)> {
    let mut labels = Vec::new();
    loop {
        s = s.trim_start_matches([' ', ',']);
        if let Some(after) = s.strip_prefix('}') {
            return Some((labels, after));
        }
        let eq = s.find('=')?;
        let key = s[..eq].trim().to_string();
        s = s[eq + 1..].strip_prefix('"')?;
        let mut value = String::new();
        let mut chars = s.char_indices();
        let end = loop {
            let (i, c) = chars.next()?;
            match c {
                '\\' => match chars.next()?.1 {
                    'n' => value.push('\n'),
                    other => value.push(other),
                },
                '"' => break i,
                _ => value.push(c),
            }
        };
        labels.push((key, value));
        s = &s[end + 1..];
    }
}
