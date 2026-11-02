//! Songbook's player: what a listener sees. The library by category, artist and album, search, playlists, the songs kept on
//! the device, and the bar that plays them. The app does no I/O: it asks its host for what it shows (`live::Request`) and is
//! told what came (`live::Msg`); the sound is made by [`ENGINE`], a script the page loads, which the screens command and
//! hear through two hidden elements (see [`engine`]).

// Seconds and byte counts are small and only ever shown.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::too_many_lines
)]

pub mod engine;
pub mod live;
pub mod screens;

pub use live::{Msg, Player, Request, Setup, player};

/// The player's style.
pub const STYLE: &str = include_str!("style.css");

/// The audio engine, served as a file of the page.
pub const ENGINE: &str = include_str!("engine.js");

/// The service worker: keeps pictures, the player's files and its last page on the device (`{base}/sw.js`).
pub const SERVICE_WORKER: &str = include_str!("sw.js");
