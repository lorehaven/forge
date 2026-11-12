//! A tolerant GEDCOM 5.5.1 reader: BOM, CRLF/LF, `CONC`/`CONT`, unknown tags skipped.

#[derive(Debug, Default)]
pub struct Record {
    pub tag: String,
    pub xref: Option<String>,
    pub value: String,
    pub children: Vec<Self>,
}

impl Record {
    #[must_use]
    pub fn child(&self, tag: &str) -> Option<&Self> {
        self.children.iter().find(|c| c.tag == tag)
    }

    pub fn all<'a>(&'a self, tag: &'a str) -> impl Iterator<Item = &'a Self> {
        self.children.iter().filter(move |c| c.tag == tag)
    }

    #[must_use]
    pub fn value_of(&self, tag: &str) -> Option<&str> {
        self.child(tag)
            .map(|c| c.value.as_str())
            .filter(|v| !v.is_empty())
    }
}

/// Splits a file into its level-0 records, each with its nested children.
#[must_use]
pub fn parse_records(text: &str) -> Vec<Record> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    // Each open record, from the level-0 record down to the line being read.
    let mut stack: Vec<Record> = Vec::new();
    let mut top: Vec<Record> = Vec::new();
    for line in text.lines() {
        let line = line.trim_end_matches('\r').trim_start();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.splitn(2, ' ');
        let Some(level) = parts.next().and_then(|l| l.parse::<usize>().ok()) else {
            continue;
        };
        let rest = parts.next().unwrap_or("");
        let (xref, rest) = if rest.starts_with('@') {
            match rest.split_once(' ') {
                Some((x, r)) => (Some(x.trim_matches('@').to_string()), r),
                None => (Some(rest.trim_matches('@').to_string()), ""),
            }
        } else {
            (None, rest)
        };
        let (tag, value) = rest.split_once(' ').unwrap_or((rest, ""));
        match tag {
            "CONC" | "CONT" => {
                if let Some(open) = stack.last_mut() {
                    if tag == "CONT" {
                        open.value.push('\n');
                    }
                    open.value.push_str(value);
                }
                continue;
            }
            _ => {}
        }
        // Close records that are as deep as, or deeper than, the new line.
        while stack.len() > level {
            let done = stack.pop().unwrap_or_default();
            match stack.last_mut() {
                Some(parent) => parent.children.push(done),
                None => top.push(done),
            }
        }
        stack.push(Record {
            tag: tag.to_string(),
            xref,
            value: value.to_string(),
            children: Vec::new(),
        });
    }
    while let Some(done) = stack.pop() {
        match stack.last_mut() {
            Some(parent) => parent.children.push(done),
            None => top.push(done),
        }
    }
    top
}
