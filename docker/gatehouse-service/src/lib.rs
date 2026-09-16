//! Gatehouse - the estate's auth service. Owns `auth`, the only seeder/login/issuer.

// `Result<T, Response>` early-returns trip this; boxing buys nothing.
#![allow(clippy::result_large_err)]

pub mod api;
pub mod bootstrap;
pub mod catalog;
pub mod clients;
pub mod codes;
pub mod crypto;
pub mod email;
pub mod keys;
pub mod mfa;
pub mod realm;
pub mod services;
pub mod test_support;
pub mod tokens;
pub mod ui;

pub use catalog::PermissionCatalog;
pub use keys::SigningKeys;
pub use tokens::VerificationTokens;
