//! What Songbook's admin API says and the rules that name things. No I/O, and it builds for the browser: the service and
//! the browser client share it.
//!
//! * [`api`] - the wire model: what the endpoints answer and accept.
//! * [`naming`] - song identity (`key`), titles without feat./remaster noise, and the file name of a song.

pub mod api;
pub mod naming;

pub use api::{
    AddArtists, Added, Album, ArtistRow, BlacklistBody, Browse, CategoryBody, Filter, Job,
    JobStart, LinkBody, NewArtist, NewCategory, RemoveArtist, Removed, Retagged, Started, State,
    Suggestion, Track, Updated,
};
