//! Warehouse - the estate's storage service (cargo/docker registries, files, artifacts).
//! `/v2` and `/token` sit at the server root (registry protocol); everything else under `BASE_PATH`.

// Router `Result<T, Response>` early-returns trip this; boxing buys nothing.
#![allow(clippy::result_large_err)]

pub mod docker_token;
pub mod domain;
pub mod middleware;
pub mod routers;
pub mod utils;
