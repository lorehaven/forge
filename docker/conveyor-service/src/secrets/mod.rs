//! Values conveyor holds on a pipeline's behalf: [`crypto`] seals, [`store`]
//! persists, [`redact`] hides them from logs. A job sees only what it names.

pub mod crypto;
pub mod redact;
pub mod store;

pub use crypto::{CryptoError, SecretKey};
pub use redact::Redactor;
pub use store::{Scope, SecretError, SecretRef};

/// Name a repo's webhook secret is stored under; falls back to
/// `CONVEYOR_WEBHOOK_SECRET` for single-tenant deployments.
pub const WEBHOOK_SECRET_NAME: &str = "WEBHOOK_SECRET";
