//! Paging convention shared by every listing endpoint here: `n` (page size),
//! `last` (previous page's final item, exclusive) - same shape as `registry::catalog`.

/// One page of a larger, ordered sequence.
pub struct Page<T> {
    pub items: Vec<T>,
    pub has_more: bool,
}

/// A caller's `?n=`, or `default`, clamped to `max` rather than refused.
pub fn page_size(requested: Option<usize>, default: usize, max: usize) -> usize {
    requested.unwrap_or(default).clamp(1, max)
}

/// Splits ordered `items` into the leading `limit` plus `has_more`; caller fetches `limit + 1` rows.
pub fn paginate<T>(mut items: Vec<T>, limit: usize) -> Page<T> {
    let has_more = items.len() > limit;
    items.truncate(limit);
    Page { items, has_more }
}

/// Items to skip to resume after `last`; a stale/bogus cursor restarts rather than erroring.
pub fn resume_after<T>(items: &[T], last: Option<&str>, key: impl Fn(&T) -> &str) -> usize {
    match last {
        Some(last) => items
            .iter()
            .position(|item| key(item) == last)
            .map(|index| index + 1)
            .unwrap_or(0),
        None => 0,
    }
}

/// A `Link: <...>; rel="next"` header for the page after `last` (percent-encoded caller content).
pub fn next_link(path_and_query: &str, n: usize, last: &str) -> String {
    format!(
        "<{path_and_query}&n={n}&last={}>; rel=\"next\"",
        urlencoding::encode(last)
    )
}
