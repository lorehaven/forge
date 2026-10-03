//! Gantry - installs, upgrades, scales and swaps what runs in the cluster, from a UI.
//!
//! A gantry carries a hoist along a whole workshop floor: it lifts one load off, moves it and sets another
//! down, anywhere along its span. This service spans every overlay Warehouse holds as a rivet package, and
//! its signature job is taking the GPU from one workload and giving it to another. See
//! `plans/GANTRY_SERVICE.md`.

// Router `Result<T, Response>` early-returns trip this; boxing buys nothing.
#![allow(clippy::result_large_err)]

pub mod domain;
pub mod routers;
