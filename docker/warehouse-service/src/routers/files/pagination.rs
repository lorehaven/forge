//! The paging convention every listing endpoint in this module shares: `n`
//! (page size) and `last` (the previous page's final item, exclusive) - the
//! same shape `routers::docker::registry::catalog` already uses, kept rather
//! than inventing a second convention in the same service.

/// One page of a larger, ordered sequence.
pub struct Page<T> {
    pub items: Vec<T>,
    pub has_more: bool,
}

/// The page size to actually use - a caller's `?n=`, or `default`, clamped to
/// `max` either way. Nothing here is security-sensitive, so an out-of-range
/// request is corrected rather than refused.
pub fn page_size(requested: Option<usize>, default: usize, max: usize) -> usize {
    requested.unwrap_or(default).clamp(1, max)
}

/// Splits an already-ordered `items` into the leading `limit` and whether
/// anything follows them. The caller decides how many extra rows to fetch
/// beyond `limit` for this to answer `has_more` without a second query -
/// `limit + 1` is the convention used throughout this module.
pub fn paginate<T>(mut items: Vec<T>, limit: usize) -> Page<T> {
    let has_more = items.len() > limit;
    items.truncate(limit);
    Page { items, has_more }
}

/// How many leading items of an already-ordered, in-memory sequence to skip
/// to resume just after `last`: one past the matching key, or the very start
/// when `last` is `None` or matches nothing (a stale or bogus cursor restarts
/// rather than erroring) - the same recovery `routers::docker::registry::catalog`
/// takes for its own `last`.
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

/// A `Link: <...>; rel="next"` header value for `path` (already carrying its
/// own query parameters up to but not including `n`/`last`), naming the page
/// that follows `last`. `last` is percent-encoded - it is caller content
/// (a file name, a repository path), not something safe to place in a header
/// unescaped.
pub fn next_link(path_and_query: &str, n: usize, last: &str) -> String {
    format!(
        "<{path_and_query}&n={n}&last={}>; rel=\"next\"",
        urlencoding::encode(last)
    )
}
