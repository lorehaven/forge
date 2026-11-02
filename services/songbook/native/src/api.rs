//! The library API of the service, as the tree needs it: a trait, so that the tree can be tried without a server.

use quench_serde::json;
use songbook_core::{
    AlbumQuery, Home, Overview, PlaylistInfo, Search, SongInfo, SongList, SongQuery,
};

use crate::auth::{Auth, Failure};

pub type Result<T> = std::result::Result<T, Failure>;

pub trait Api: Send + Sync {
    /// `…/songbook/api/v1`, where art and streams are.
    fn base(&self) -> String;
    fn home(&self) -> Result<Home>;
    fn overview(&self) -> Result<Overview>;
    fn albums(&self, artist: &str) -> Result<Vec<songbook_core::AlbumInfo>>;
    fn songs(&self, query: &SongQuery) -> Result<Vec<SongInfo>>;
    fn search(&self, text: &str) -> Result<Search>;
    /// What this person played last on any device.
    fn recent(&self) -> Result<Vec<SongInfo>>;
    fn playlists(&self) -> Result<Vec<PlaylistInfo>>;
    /// Tells the service a song was started.
    fn played(&self, song: i64) -> Result<()>;
}

fn parse<T: quench_serde::Deserialize>(text: &str) -> Result<T> {
    json::from_str(text).map_err(|e| Failure::Unreachable(format!("not an answer: {e}")))
}

impl Api for Auth {
    fn base(&self) -> String {
        self.api()
    }

    fn home(&self) -> Result<Home> {
        parse(&self.get("/library/home")?)
    }

    fn overview(&self) -> Result<Overview> {
        parse(&self.get("/library/overview")?)
    }

    fn albums(&self, artist: &str) -> Result<Vec<songbook_core::AlbumInfo>> {
        let query = AlbumQuery {
            artist: artist.to_string(),
            ..AlbumQuery::default()
        };
        parse(&self.get(&format!("/library/albums{}", query.query()))?)
    }

    fn songs(&self, query: &SongQuery) -> Result<Vec<SongInfo>> {
        Ok(parse::<SongList>(&self.get(&format!("/library/songs{}", query.query()))?)?.songs)
    }

    fn search(&self, text: &str) -> Result<Search> {
        parse(&self.get(&format!(
            "/library/search?q={}",
            songbook_core::api::encode(text)
        ))?)
    }

    fn recent(&self) -> Result<Vec<SongInfo>> {
        Ok(parse::<SongList>(&self.get("/library/recent?limit=40")?)?.songs)
    }

    fn playlists(&self) -> Result<Vec<PlaylistInfo>> {
        parse(&self.get("/playlists")?)
    }

    fn played(&self, song: i64) -> Result<()> {
        self.post(
            "/history",
            &json::to_string(&songbook_core::PlayedBody { song_id: song }),
        )
        .map(|_| ())
    }
}
