//! What every handler and job shares.

use crate::config::Config;
use crate::deezer::Deezer;
use crate::index::Index;
use crate::jobs::Jobs;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct App {
    pub config: Arc<Config>,
    pub index: Index,
    pub jobs: Jobs,
    pub deezer: Deezer,
}
