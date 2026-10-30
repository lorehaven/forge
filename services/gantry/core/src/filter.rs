//! Which packages the home page lists, and how many there are of each state.

use crate::model::{Group, SYNC_STATES};

/// The packages that pass the filter, those needing attention first: a sync state (empty for every one) and a piece
/// of the name.
#[must_use]
pub fn matching(groups: &[Group], sync: &str, query: &str) -> Vec<Group> {
    let query = query.trim().to_ascii_lowercase();
    let mut found: Vec<Group> = groups
        .iter()
        .filter(|g| sync.is_empty() || g.summary.sync == sync)
        .filter(|g| query.is_empty() || g.package.to_ascii_lowercase().contains(&query))
        .cloned()
        .collect();
    found.sort_by_key(|g| (g.summary.sync == "synced", g.package.clone()));
    found
}

/// The chips above the cards: each sync state that has packages (or is the one chosen), with its count.
#[must_use]
pub fn chips(groups: &[Group], chosen: &str) -> Vec<(&'static str, usize)> {
    SYNC_STATES
        .into_iter()
        .map(|state| {
            (
                state,
                groups.iter().filter(|g| g.summary.sync == state).count(),
            )
        })
        .filter(|(state, n)| *n > 0 || *state == chosen)
        .collect()
}
