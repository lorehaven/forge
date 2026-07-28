//! Values conveyor holds on a pipeline's behalf.
//!
//! Three parts, deliberately separate: [`crypto`] knows how to seal a value and
//! nothing about where it lives, [`store`] knows where it lives and nothing
//! about the cipher, and [`redact`] knows only that certain strings must not
//! appear in a log.
//!
//! A job sees a secret only if it names it. That is the whole access model:
//! `secrets = ["KUBE_TOKEN"]` in the pipeline, and nothing else in the store is
//! visible to that job.

pub mod crypto;
pub mod redact;
pub mod store;

pub use crypto::{CryptoError, SecretKey};
pub use redact::Redactor;
pub use store::{Scope, SecretError, SecretRef};

/// The name a repository's webhook secret is stored under.
///
/// A repository with one signs its deliveries with that; one without falls back
/// to `CONVEYOR_WEBHOOK_SECRET`, which is what a single-tenant deployment needs
/// and all that existed before the store did.
pub const WEBHOOK_SECRET_NAME: &str = "WEBHOOK_SECRET";
