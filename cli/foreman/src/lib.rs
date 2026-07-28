//! Foreman - the local development estate, described in TOML rather than in a
//! shell script.
//!
//! A project drops a `foreman.toml` at its root naming its services, the
//! containers underneath them and the tasks that have to run before either.
//! Foreman starts what was asked for plus what it cannot start without, keeps a
//! pid per service so `stop` can actually stop it, and gets out of the way.

pub mod cli;
pub mod commands;
pub mod config;
pub mod docker;
pub mod estate;
pub mod process;
pub mod repl;
pub mod services;
pub mod tasks;
pub mod ui;
pub mod vars;
