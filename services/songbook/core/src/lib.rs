//! What Songbook's admin API says and the rules that name things. No I/O, and it builds for the browser: the service and
//! the browser client share it.
//!
//! * [`api`] - the wire model: what the admin endpoints answer and accept.
//! * [`listen`] - the same for the listener's endpoints (library, streaming, playlists).
//! * [`naming`] - song identity (`key`), titles without feat./remaster noise, and the file name of a song.

pub mod api;
pub mod listen;
pub mod naming;

pub use api::{
    AddArtists, Added, Album, ArtistRow, BlacklistBody, Browse, CategoryBody, Filter, GenresBody,
    Job, JobStart, LinkBody, NewArtist, NewCategory, RemoveArtist, Removed, RenameCategory,
    Retagged, Schedule, Started, State, Stats, Suggestion, TopTrack, Track, Updated, popularity,
};
pub use listen::{
    AlbumInfo, AlbumQuery, ArtistInfo, CategoryInfo, Home, Mix, Overview, PlaylistBody,
    PlaylistInfo, Search, SearchQuery, SongIds, SongIdsBody, SongInfo, SongList, SongQuery,
};
