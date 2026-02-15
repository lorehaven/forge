use crate::core::indexer::Indexer;
use anyhow::{Context, Result};
use std::path::Path;

pub struct CodingAgent {
    indexer: Indexer,
}

impl CodingAgent {
    pub fn new(index_path: &Path) -> Result<Self> {
        let indexer = Indexer::new(index_path)?;
        Ok(Self { indexer })
    }

    pub fn index_project(&self, project_root: &Path) -> Result<()> {
        self.indexer.index_project(project_root)
    }

    pub fn search(&self, query_str: &str, limit: usize) -> Result<Vec<(String, String)>> {
        self.indexer.search(query_str, limit)
    }
}
