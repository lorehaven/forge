//! The navigation of a book, as just-the-docs builds it from the front matter: a page names its `parent` by
//! title (with `grand_parent` when two pages with children share that title), siblings are ordered by
//! `nav_order`, then the pages without one by title, case aside.

use crate::page::Page;
use std::collections::BTreeMap;

/// The navigation tree over a book's pages, by index into its page list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Nav {
    roots: Vec<usize>,
    children: Vec<Vec<usize>>,
    parents: Vec<Option<usize>>,
}

impl Nav {
    /// The tree of `pages`. A page without a title, with `nav_exclude: true`, or whose parent cannot be found
    /// (no page with children has that title, or it is ambiguous) is not in the navigation, as on the site.
    #[must_use]
    pub fn of(pages: &[Page]) -> Self {
        let mut nav = Self {
            roots: Vec::new(),
            children: vec![Vec::new(); pages.len()],
            parents: vec![None; pages.len()],
        };
        let shown = |page: &Page| page.title.is_some() && !page.nav_exclude;
        for (i, page) in pages.iter().enumerate() {
            if !shown(page) {
                continue;
            }
            match &page.parent {
                None => nav.roots.push(i),
                Some(parent) => {
                    if let Some(p) = parent_of(pages, parent, page.grand_parent.as_deref())
                        && shown(&pages[p])
                    {
                        nav.children[p].push(i);
                        nav.parents[i] = Some(p);
                    }
                }
            }
        }
        let order = |list: &mut Vec<usize>| {
            list.sort_by(|&a, &b| {
                let (a, b) = (&pages[a], &pages[b]);
                match (a.nav_order, b.nav_order) {
                    (Some(x), Some(y)) => x.cmp(&y),
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => std::cmp::Ordering::Equal,
                }
                .then_with(|| a.label().to_lowercase().cmp(&b.label().to_lowercase()))
                .then_with(|| a.path.cmp(&b.path))
            });
        };
        order(&mut nav.roots);
        for list in &mut nav.children {
            order(list);
        }
        nav
    }

    /// The top-level pages, in order.
    #[must_use]
    pub fn roots(&self) -> &[usize] {
        &self.roots
    }

    /// The children of page `i`, in order.
    #[must_use]
    pub fn children(&self, i: usize) -> &[usize] {
        self.children.get(i).map_or(&[], Vec::as_slice)
    }

    /// The parent of page `i` in the navigation.
    #[must_use]
    pub fn parent(&self, i: usize) -> Option<usize> {
        self.parents.get(i).copied().flatten()
    }

    /// Page `i` and its ancestors up to the top level, the page last: a breadcrumb.
    #[must_use]
    pub fn trail(&self, i: usize) -> Vec<usize> {
        let mut trail = vec![i];
        let mut at = i;
        while let Some(p) = self.parent(at) {
            // A parent cycle cannot be built by `of`, but stop anyway rather than loop.
            if trail.contains(&p) {
                break;
            }
            trail.push(p);
            at = p;
        }
        trail.reverse();
        trail
    }

    /// Every page in the navigation in reading order (depth first), with its depth (0 at the top level).
    #[must_use]
    pub fn walk(&self) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        let mut stack: Vec<(usize, usize)> = self.roots.iter().rev().map(|&r| (r, 0)).collect();
        while let Some((i, depth)) = stack.pop() {
            out.push((i, depth));
            for &c in self.children(i).iter().rev() {
                stack.push((c, depth + 1));
            }
        }
        out
    }

    /// The pages before and after page `i` in reading order: "previous" and "next".
    #[must_use]
    pub fn neighbours(&self, i: usize) -> (Option<usize>, Option<usize>) {
        let order = self.walk();
        let Some(at) = order.iter().position(|&(p, _)| p == i) else {
            return (None, None);
        };
        let before = at.checked_sub(1).map(|k| order[k].0);
        let after = order.get(at + 1).map(|&(p, _)| p);
        (before, after)
    }
}

/// The page with children titled `parent`, narrowed by `grand_parent` when several have that title.
fn parent_of(pages: &[Page], parent: &str, grand_parent: Option<&str>) -> Option<usize> {
    let candidates: Vec<usize> = pages
        .iter()
        .enumerate()
        .filter(|(_, p)| p.has_children && p.title.as_deref() == Some(parent))
        .map(|(i, _)| i)
        .collect();
    match candidates.as_slice() {
        [one] => Some(*one),
        [] => None,
        many => {
            let narrowed: Vec<usize> = many
                .iter()
                .copied()
                .filter(|&c| grand_parent.is_some() && pages[c].parent.as_deref() == grand_parent)
                .collect();
            (narrowed.len() == 1).then(|| narrowed[0])
        }
    }
}

/// A parent, its parent, and a `nav_order`: the pages that share all three are in no defined order.
type Siblings<'a> = (Option<&'a str>, Option<&'a str>, i64);

/// What is wrong with the navigation: pages without a title, parents that are not pages with children or are
/// ambiguous, and siblings sharing a `nav_order`.
#[must_use]
pub fn nav_problems(pages: &[Page]) -> Vec<String> {
    let mut out = Vec::new();
    let mut by_title: BTreeMap<&str, Vec<&Page>> = BTreeMap::new();
    for page in pages {
        match &page.title {
            Some(title) => by_title.entry(title.as_str()).or_default().push(page),
            None => out.push(format!("{}: no `title` in the front matter", page.path)),
        }
    }
    for page in pages {
        let Some(parent) = page.parent.as_deref() else {
            continue;
        };
        let candidates: Vec<&&Page> = by_title
            .get(parent)
            .map(|c| c.iter().filter(|p| p.has_children).collect())
            .unwrap_or_default();
        match candidates.len() {
            0 => {
                let any = by_title.contains_key(parent);
                out.push(if any {
                    format!(
                        "{}: parent `{parent}` exists but has no `has_children: true`",
                        page.path
                    )
                } else {
                    format!(
                        "{}: parent `{parent}` is not the title of any page",
                        page.path
                    )
                });
            }
            1 => {}
            _ => {
                let narrowed = candidates
                    .iter()
                    .filter(|p| p.parent == page.grand_parent)
                    .count();
                if page.grand_parent.is_none() || narrowed != 1 {
                    out.push(format!(
                        "{}: parent `{parent}` is ambiguous ({} pages with that title have children): add `grand_parent`",
                        page.path,
                        candidates.len()
                    ));
                }
            }
        }
    }
    // Siblings with the same `nav_order` are shown in no defined order.
    let mut groups: BTreeMap<Siblings<'_>, Vec<&str>> = BTreeMap::new();
    for page in pages {
        if let Some(order) = page.nav_order {
            groups
                .entry((page.parent.as_deref(), page.grand_parent.as_deref(), order))
                .or_default()
                .push(&page.path);
        }
    }
    for ((parent, _, order), paths) in groups {
        if paths.len() > 1 {
            out.push(format!(
                "nav_order {order} is used by {} pages under {}: {}",
                paths.len(),
                parent.unwrap_or("the top level"),
                paths.join(", ")
            ));
        }
    }
    out
}
