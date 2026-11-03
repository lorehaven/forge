//! Songbook - the library of every song. This crate is the admin module: the index of songs, artists, albums and tags,
//! the Deezer sync that fills it, the YouTube lookup, download and conversion that give each song a file, and the HTTP
//! API and browser client that drive them.
//!
//! One index (`meta/library.db`, SQLite) is the single source of truth and this service its single writer. Every song is
//! one row however many albums it is on, and one file in the flat audio folder, named `<artists A-Z> - <title>`.

// Router `Result<T, Response>` early-returns trip this; boxing buys nothing.
#![allow(clippy::result_large_err)]
// Counts and durations are small and rounded for display or protocol use.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::too_many_lines,
    clippy::significant_drop_tightening,
    clippy::similar_names,
    clippy::implicit_hasher
)]
// Documentation prose: product names and long module summaries are fine.
#![allow(clippy::doc_markdown, clippy::too_long_first_doc_paragraph)]

pub mod app;
pub mod config;
pub mod convert;
pub mod deezer;
pub mod download;
pub mod files;
pub mod images;
pub mod index;
pub mod jobs;
pub mod library;
pub mod pipeline;
pub mod playlists;
pub mod purge;
pub mod replace;
pub mod routers;
pub mod schedule;
pub mod similar;
pub mod sync;
pub mod youtube;
pub mod ytdlp;
