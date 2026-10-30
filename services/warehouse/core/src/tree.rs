//! The trees the pages draw: docker repositories, a storage's files, and what a selection in a registry resolves to.

use std::collections::BTreeMap;

use crate::model::{ArtifactEntry, BrowseFile, CrateVersion, RivetEntry};

// ── Docker repositories ──────────────────────────────────────────────────────

/// A repository path (`team/app`) split at its slashes. A node is a repository when `repo` is set; it may also be a folder.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RepoNode {
    pub children: BTreeMap<String, RepoNode>,
    pub repo: Option<String>,
}

#[must_use]
pub fn repo_tree(repos: &[String]) -> RepoNode {
    let mut root = RepoNode::default();
    for repo in repos {
        let mut node = &mut root;
        for segment in repo.split('/') {
            node = node.children.entry(segment.to_string()).or_default();
        }
        node.repo = Some(repo.clone());
    }
    root
}

impl RepoNode {
    /// Whether this node or something below it is the repository `selected`.
    #[must_use]
    pub fn holds(&self, selected: &str) -> bool {
        self.repo.as_deref() == Some(selected) || self.children.values().any(|c| c.holds(selected))
    }
}

// ── A storage's files ────────────────────────────────────────────────────────

/// A directory or a file of a storage.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileNode {
    pub children: BTreeMap<String, FileNode>,
    pub is_file: bool,
    pub size: Option<i64>,
}

/// What `?path=` names within the tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selection {
    /// A directory: the storage root when the path is empty.
    Dir(String),
    File {
        path: String,
        size: Option<i64>,
    },
    /// Nothing in the (possibly capped) tree.
    Missing(String),
}

impl Selection {
    #[must_use]
    pub fn path(&self) -> &str {
        match self {
            Self::Dir(path) | Self::File { path, .. } | Self::Missing(path) => path,
        }
    }
}

#[must_use]
pub fn file_tree(files: &[BrowseFile]) -> FileNode {
    let mut root = FileNode::default();
    for file in files {
        let segments: Vec<&str> = file.path.split('/').filter(|s| !s.is_empty()).collect();
        let mut node = &mut root;
        for (index, segment) in segments.iter().enumerate() {
            node = node.children.entry((*segment).to_string()).or_default();
            if index + 1 == segments.len() {
                node.is_file = true;
                node.size = file.size;
            }
        }
    }
    root
}

/// Trim, drop `.`, `..` and empty segments, rejoin with `/`: a tidy-up, not the security boundary (the service resolves).
#[must_use]
pub fn normalize_path(raw: &str) -> String {
    raw.split('/')
        .filter(|segment| !segment.is_empty() && *segment != "." && *segment != "..")
        .collect::<Vec<_>>()
        .join("/")
}

/// The directory a path sits in; empty for a top-level one.
#[must_use]
pub fn parent_dir(path: &str) -> String {
    path.rsplit_once('/')
        .map(|(parent, _)| parent.to_string())
        .unwrap_or_default()
}

/// The last segment, or the whole path when there is none.
#[must_use]
pub fn last_segment(path: &str) -> &str {
    path.rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(path)
}

impl FileNode {
    #[must_use]
    pub fn node_at(&self, path: &str) -> Option<&FileNode> {
        let mut node = self;
        for segment in path.split('/').filter(|s| !s.is_empty()) {
            node = node.children.get(segment)?;
        }
        Some(node)
    }

    #[must_use]
    pub fn resolve(&self, path: &str) -> Selection {
        if path.is_empty() {
            return Selection::Dir(String::new());
        }
        match self.node_at(path) {
            None => Selection::Missing(path.to_string()),
            Some(node) if node.is_leaf() => Selection::File {
                path: path.to_string(),
                size: node.size,
            },
            Some(_) => Selection::Dir(path.to_string()),
        }
    }

    /// A file with nothing below it.
    #[must_use]
    pub fn is_leaf(&self) -> bool {
        self.is_file && self.children.is_empty()
    }

    /// Directories first, then files, each group by name.
    #[must_use]
    pub fn sorted_children(&self) -> Vec<(&String, &FileNode)> {
        let mut children: Vec<_> = self.children.iter().collect();
        children.sort_by(|(a_name, a), (b_name, b)| {
            a.is_file.cmp(&b.is_file).then_with(|| a_name.cmp(b_name))
        });
        children
    }

    /// Whether anything below is a directory (a leaf file does not make a folder expandable).
    #[must_use]
    pub fn has_dirs(&self) -> bool {
        self.children.values().any(|c| !c.is_leaf())
    }
}

// ── Registries ───────────────────────────────────────────────────────────────

/// Which version of a crate is open: the one asked for if it exists, else the newest not yanked, else the newest.
/// `versions` are oldest first, as the index has them.
#[must_use]
pub fn active_crate_version(versions: &[CrateVersion], wanted: Option<&str>) -> Option<usize> {
    wanted
        .and_then(|w| versions.iter().position(|v| v.vers == w))
        .or_else(|| versions.iter().rposition(|v| !v.yanked))
        .or_else(|| versions.len().checked_sub(1))
}

/// program, then platform, then its versions, newest `version_code` first.
pub type ArtifactTree = BTreeMap<String, BTreeMap<String, Vec<ArtifactEntry>>>;

#[must_use]
pub fn artifact_tree(entries: &[ArtifactEntry]) -> ArtifactTree {
    let mut tree: ArtifactTree = BTreeMap::new();
    for entry in entries {
        tree.entry(entry.program.clone())
            .or_default()
            .entry(entry.platform.clone())
            .or_default()
            .push(entry.clone());
    }
    for platforms in tree.values_mut() {
        for list in platforms.values_mut() {
            list.sort_by_key(|v| std::cmp::Reverse(v.version_code));
        }
    }
    tree
}

/// What an artifact selection resolves to: the program and platform that exist, and the open version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactPick<'a> {
    pub program: Option<&'a str>,
    pub platform: Option<&'a str>,
    pub selected: Option<&'a ArtifactEntry>,
}

/// An unknown program selects nothing; an unknown platform falls to the program's first; an unknown code to the newest.
#[must_use]
pub fn pick_artifact<'a>(
    tree: &'a ArtifactTree,
    program: Option<&str>,
    platform: Option<&str>,
    code: Option<i64>,
) -> ArtifactPick<'a> {
    let program_entry = program.and_then(|p| tree.get_key_value(p));
    let platforms = program_entry.map(|(_, platforms)| platforms);
    let platform_entry = platform
        .and_then(|p| platforms.and_then(|m| m.get_key_value(p)))
        .or_else(|| platforms.and_then(|m| m.iter().next()));
    let versions: &[ArtifactEntry] = platform_entry.map_or(&[], |(_, list)| list.as_slice());
    let selected = code
        .and_then(|code| versions.iter().find(|v| v.version_code == code))
        .or_else(|| versions.first());
    ArtifactPick {
        program: program_entry.map(|(name, _)| name.as_str()),
        platform: platform_entry.map(|(name, _)| name.as_str()),
        selected,
    }
}

/// name, then its versions in the order the service sent them (newest first).
pub type RivetTree = BTreeMap<String, Vec<RivetEntry>>;

#[must_use]
pub fn rivet_tree(entries: &[RivetEntry]) -> RivetTree {
    let mut tree: RivetTree = BTreeMap::new();
    for entry in entries {
        tree.entry(entry.name.clone())
            .or_default()
            .push(entry.clone());
    }
    tree
}

/// The version of a package that is open: the one asked for, else the newest not yanked (what `latest` would give), else the
/// newest.
#[must_use]
pub fn pick_rivet<'a>(versions: &'a [RivetEntry], wanted: Option<&str>) -> Option<&'a RivetEntry> {
    wanted
        .and_then(|w| versions.iter().find(|p| p.version == w))
        .or_else(|| versions.iter().find(|p| !p.yanked))
        .or_else(|| versions.first())
}
