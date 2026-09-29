//! Gatehouse - the estate's auth service. Owns `auth`, the only seeder/login/issuer.

// `Result<T, Response>` early-returns trip this; boxing buys nothing.
#![allow(clippy::result_large_err)]

pub mod api;
pub mod avatar;
pub mod bootstrap;
pub mod catalog;
pub mod clients;
pub mod codes;
pub mod crypto;
pub mod email;
pub mod email_change;
pub mod invites;
pub mod keys;
pub mod links;
pub mod mfa;
pub mod notices;
pub mod notify;
pub mod ratelimit;
pub mod realm;
pub mod services;
pub mod test_support;
pub mod tokens;
pub mod ui;

pub use catalog::PermissionCatalog;
pub use keys::SigningKeys;
pub use links::PublicBase;
pub use ratelimit::RateLimiter;
pub use tokens::VerificationTokens;
