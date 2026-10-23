//! Paging convention shared by every listing endpoint here: `n` (page size),
//! `last` (previous page's final item, exclusive) - same shape as `registry::catalog`.

/// One page of a larger, ordered sequence.
#[derive(Debug)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub has_more: bool,
}

/// A caller's `?n=`, or `default`, clamped to `max` rather than refused.
#[must_use]
pub fn page_size(requested: Option<usize>, default: usize, max: usize) -> usize {
    requested.unwrap_or(default).clamp(1, max)
}

/// Splits ordered `items` into the leading `limit` plus `has_more`; caller fetches `limit + 1` rows.
#[must_use]
pub fn paginate<T>(mut items: Vec<T>, limit: usize) -> Page<T> {
    let has_more = items.len() > limit;
    items.truncate(limit);
    Page { items, has_more }
}

/// Items to skip to resume after `last`; a stale/bogus cursor restarts rather than erroring.
pub fn resume_after<T>(items: &[T], last: Option<&str>, key: impl Fn(&T) -> &str) -> usize {
    last.map_or(0, |last| {
        items
            .iter()
            .position(|item| key(item) == last)
            .map_or(0, |index| index + 1)
    })
}

/// A `Link: <...>; rel="next"` header for the page after `last` (percent-encoded caller content).
#[must_use]
pub fn next_link(path_and_query: &str, n: usize, last: &str) -> String {
    format!(
        "<{path_and_query}&n={n}&last={}>; rel=\"next\"",
        quench_serde::urlencoded::percent_encode(last)
    )
}
