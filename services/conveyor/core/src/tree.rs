//! The organisational tree: paths, breadcrumbs, and the home page's nesting of projects and repositories.

use crate::domain::{Project, Repo};
use std::collections::HashMap;

/// `root/.../leaf`, read from the list in memory.
#[must_use]
pub fn project_path(id: &str, all: &[Project]) -> String {
    let mut names = Vec::new();
    let mut current = all.iter().find(|p| p.id == id);
    while let Some(project) = current {
        names.push(project.name.as_str());
        current = project
            .parent_id
            .as_deref()
            .and_then(|parent_id| all.iter().find(|p| p.id == parent_id));
    }
    names.reverse();
    names.join("/")
}

/// `id`'s ancestors, root first, `id` itself last. Empty if `id` names no project.
#[must_use]
pub fn ancestor_chain<'a>(id: &str, all: &'a [Project]) -> Vec<&'a Project> {
    let by_id: HashMap<&str, &Project> = all.iter().map(|p| (p.id.as_str(), p)).collect();

    let mut chain = Vec::new();
    let mut current = by_id.get(id).copied();
    while let Some(project) = current {
        // A parent loop (a hand-edited database) must not spin forever.
        if chain.iter().any(|seen: &&Project| seen.id == project.id) {
            break;
        }
        chain.push(project);
        current = project
            .parent_id
            .as_deref()
            .and_then(|parent_id| by_id.get(parent_id).copied());
    }

    chain.reverse();
    chain
}

/// One entry of a level of the tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    /// The repositories of the leaf projects of the level, in one table.
    Repos(Vec<Repo>),
    /// A project with nothing in it: a plain link.
    Empty(Project),
    /// A project with children (and rarely repositories of its own).
    Container {
        project: Project,
        repos: Vec<Repo>,
        children: Vec<Node>,
    },
}

/// The tree rooted at `root` (the top when `None`); `root` itself is not part of it, but a repository attached to it directly is
/// the first entry.
#[must_use]
pub fn build(all: &[Project], repos: &[Repo], root: Option<&str>) -> Vec<Node> {
    let mut children_of: HashMap<Option<&str>, Vec<&Project>> = HashMap::new();
    for project in all {
        children_of
            .entry(project.parent_id.as_deref())
            .or_default()
            .push(project);
    }
    let mut repos_of: HashMap<&str, Vec<&Repo>> = HashMap::new();
    for repo in repos {
        repos_of
            .entry(repo.project_id.as_str())
            .or_default()
            .push(repo);
    }

    let mut out = Vec::new();
    if let Some(id) = root
        && let Some(own) = repos_of.get(id)
    {
        out.push(Node::Repos(own.iter().map(|r| (*r).clone()).collect()));
    }
    let top = children_of.get(&root).cloned().unwrap_or_default();
    out.extend(level(&top, &children_of, &repos_of, 0));
    out
}

/// A tree deeper than this is a cycle in a hand-edited database.
const MAX_DEPTH: usize = 64;

fn level(
    nodes: &[&Project],
    children_of: &HashMap<Option<&str>, Vec<&Project>>,
    repos_of: &HashMap<&str, Vec<&Repo>>,
    depth: usize,
) -> Vec<Node> {
    let mut leaf_repos = Vec::new();
    let mut empty_leaves = Vec::new();
    let mut containers = Vec::new();

    for &node in nodes {
        let kids = children_of
            .get(&Some(node.id.as_str()))
            .cloned()
            .unwrap_or_default();
        if !kids.is_empty() && depth < MAX_DEPTH {
            containers.push(Node::Container {
                project: node.clone(),
                repos: repos_of
                    .get(node.id.as_str())
                    .map(|own| own.iter().map(|r| (*r).clone()).collect())
                    .unwrap_or_default(),
                children: level(&kids, children_of, repos_of, depth + 1),
            });
        } else if let Some(own) = repos_of.get(node.id.as_str()) {
            leaf_repos.extend(own.iter().map(|r| (*r).clone()));
        } else {
            empty_leaves.push(node.clone());
        }
    }

    let mut out = Vec::new();
    if !leaf_repos.is_empty() {
        out.push(Node::Repos(leaf_repos));
    }
    out.extend(empty_leaves.into_iter().map(Node::Empty));
    out.extend(containers);
    out
}
