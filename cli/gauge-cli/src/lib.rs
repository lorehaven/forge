//! Gauge in a terminal: the hosts, their alerts and their disks, refreshed every few seconds and driven by keys.
//!
//! `state` is what is on the screen and what a key does to it, `view` draws it with `forge-tui` (the same styling as
//! `forge-toolbox`), and `source` signs in and fetches. Only `source` and the binary touch the network or the terminal.

pub mod config;
pub mod pages;
pub mod source;
pub mod state;
pub mod tiles;
pub mod view;

/// How often the numbers are fetched again.
pub const REFRESH_SECS: u64 = 5;
