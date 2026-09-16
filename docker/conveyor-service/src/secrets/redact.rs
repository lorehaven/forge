//! Strips injected secrets from stored step output before it's recorded.
//! A backstop against accidents, not a guarantee - encoding a secret evades it.

/// Shortest value worth replacing - below this, redaction does more harm than
/// good (e.g. masking every `a`). Shorter values are rejected on write.
pub const MIN_REDACTABLE: usize = 4;

/// What replaces a secret.
const MASK: &str = "••••••";

#[derive(Clone, Debug, Default)]
pub struct Redactor {
    /// Longest first, so a value that contains another is replaced whole rather
    /// than being half-masked by its own substring.
    values: Vec<String>,
}

impl Redactor {
    pub fn new(values: impl IntoIterator<Item = impl Into<String>>) -> Self {
        let mut values: Vec<String> = values
            .into_iter()
            .map(Into::into)
            .filter(|value| value.chars().count() >= MIN_REDACTABLE)
            .collect();

        values.sort_by_key(|value| std::cmp::Reverse(value.len()));
        values.dedup();
        Self { values }
    }

    /// A redactor with nothing to hide, for a job that declared no secrets.
    pub fn none() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Replaces every occurrence of every value.
    pub fn apply(&self, line: &str) -> String {
        if self.values.is_empty() {
            return line.to_string();
        }

        let mut result = line.to_string();
        for value in &self.values {
            if result.contains(value.as_str()) {
                result = result.replace(value.as_str(), MASK);
            }
        }
        result
    }

    /// Whether any value survives in `line`. Used by the tests, and cheap
    /// enough to assert with.
    pub fn leaks(&self, line: &str) -> bool {
        self.values
            .iter()
            .any(|value| line.contains(value.as_str()))
    }
}
