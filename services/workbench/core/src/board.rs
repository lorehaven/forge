//! The board: an issue per card, a column per status.

use crate::model::{Issue, STATUSES};

/// The i18n key of a status's column title: `in-progress` is `ui_status_in_progress`.
#[must_use]
pub fn status_key(status: &str) -> String {
    format!("ui_status_{}", status.replace('-', "_"))
}

/// Every status with its issues, in the workflow's order and the issues' order (by number).
#[must_use]
pub fn columns(issues: &[Issue]) -> Vec<(&'static str, Vec<&Issue>)> {
    STATUSES
        .iter()
        .map(|status| {
            let mut cards: Vec<&Issue> = issues.iter().filter(|i| i.status == *status).collect();
            cards.sort_by_key(|i| i.seq);
            (*status, cards)
        })
        .collect()
}

/// The issues with `id` moved to `status`; `None` when it is not there or already is.
#[must_use]
pub fn moved(issues: &[Issue], id: &str, status: &str) -> Option<Vec<Issue>> {
    let at = issues.iter().position(|i| i.id == id)?;
    if issues[at].status == status {
        return None;
    }
    let mut next = issues.to_vec();
    next[at].status = status.to_string();
    Some(next)
}

/// `2024-03-05T10:20:30.123Z` as `2024-03-05 10:20`, for the little a page shows of a time.
#[must_use]
pub fn short_time(rfc3339: &str) -> String {
    let (date, time) = rfc3339.split_once('T').unwrap_or((rfc3339, ""));
    let time: String = time.chars().take(5).collect();
    if time.is_empty() {
        date.to_string()
    } else {
        format!("{date} {time}")
    }
}
