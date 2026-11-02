//! What every handler and job shares.

use crate::config::Config;
use crate::deezer::Deezer;
use crate::index::Index;
use crate::jobs::Jobs;
use crate::library::Library;
use crate::playlists::Playlists;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct App {
    pub config: Arc<Config>,
    pub index: Index,
    pub jobs: Jobs,
    pub deezer: Deezer,
    /// What a listener sees: the songs that have a file.
    pub library: Library,
    /// The listeners' own playlists.
    pub playlists: Playlists,
}
