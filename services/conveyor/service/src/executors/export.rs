//! Files a job hands back through its log: what the kubernetes executor reassembles from
//! [`FILE_MARKER`](crate::executors::manifest::FILE_MARKER) lines and writes into conveyor's workspace.
//!
//! A job's pod has a checkout of its own, so the artifacts it declares and the test reports it leaves would
//! otherwise never be where conveyor looks for them. Only the paths asked for are accepted: the job's code is
//! not trusted to choose what lands in conveyor's workspace.

use crate::executors::manifest::MAX_EXPORT_BYTES;
use quench_core::base64;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path};

/// Base64 grows 4 bytes for every 3, plus a little padding.
const MAX_ENCODED: usize = MAX_EXPORT_BYTES / 3 * 4 + 8;

/// The files of one job, as they arrive.
#[derive(Debug, Default)]
pub struct Exports {
    allowed: BTreeSet<String>,
    encoded: BTreeMap<String, String>,
    /// Paths that grew past the limit; whatever else arrives for them is dropped.
    oversized: BTreeSet<String>,
}

impl Exports {
    #[must_use]
    pub fn new(paths: &[String]) -> Self {
        Self {
            allowed: paths.iter().cloned().collect(),
            ..Self::default()
        }
    }

    /// Whether anything can arrive at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.allowed.is_empty()
    }

    /// One marker line without its marker: `<path>\t<base64>`. Lines for a path nobody asked for are dropped.
    pub fn accept(&mut self, rest: &str) {
        let Some((path, chunk)) = rest.trim_end().split_once('\t') else {
            return;
        };
        if !self.allowed.contains(path) || self.oversized.contains(path) {
            return;
        }
        let encoded = self.encoded.entry(path.to_string()).or_default();
        if encoded.len() + chunk.len() > MAX_ENCODED {
            self.encoded.remove(path);
            self.oversized.insert(path.to_string());
            return;
        }
        encoded.push_str(chunk);
    }

    /// Writes every file received under `root`; returns what could not be (for the log of conveyor itself).
    pub fn write(self, root: &Path) -> Vec<String> {
        let mut problems: Vec<String> = self
            .oversized
            .iter()
            .map(|path| format!("`{path}` is over the {MAX_EXPORT_BYTES}-byte limit"))
            .collect();

        for (path, encoded) in self.encoded {
            if !Path::new(&path)
                .components()
                .all(|component| matches!(component, Component::Normal(_)))
            {
                problems.push(format!("`{path}` is not inside the checkout"));
                continue;
            }
            let Ok(bytes) = base64::STANDARD.decode(&encoded) else {
                problems.push(format!("`{path}` arrived damaged"));
                continue;
            };
            let target = root.join(&path);
            let written = target
                .parent()
                .map_or(Ok(()), std::fs::create_dir_all)
                .and_then(|()| std::fs::write(&target, bytes));
            if let Err(error) = written {
                problems.push(format!("could not write `{path}`: {error}"));
            }
        }
        problems
    }
}
