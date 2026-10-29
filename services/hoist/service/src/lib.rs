//! The public catalogue: the apps of F-Droid, IzzyOnDroid and Aptoide merged into one row per package, kept in
//! Postgres, refreshed every hour, and served (behind the realm's login) to the app's Public tab.
//!
//! `connectors` fetch, `merge` decides one row per package, `store` keeps them, `refresh` ties the three together on
//! a timer and `routers` serves what is kept.

pub mod config;
pub mod connectors;
pub mod merge;
pub mod models;
pub mod refresh;
pub mod routers;
pub mod store;
