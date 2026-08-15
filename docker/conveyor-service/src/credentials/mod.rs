//! Git authentication conveyor holds on a repository's or a project's behalf.
//!
//! Sibling to [`crate::secrets`], not a variant of it: a pipeline secret is
//! named and injected only into the job that asks for it by name, while a
//! credential is never named by a pipeline at all - it is resolved
//! automatically, once, by the checkout that needs it, for exactly the
//! repository being cloned. Keeping them apart keeps that difference obvious
//! rather than folding a second access model into one table with a
//! discriminator column.
//!
//! Sealed with its own key (`CONVEYOR_CREDENTIAL_KEY`), read through the same
//! [`crate::secrets::crypto::SecretKey`] cipher `secrets` uses - the cipher
//! wiring is not secrets-specific, only the key it is given is. A deployment
//! can rotate or lose one without touching the other.
//!
//! As with `secrets`, there is no endpoint that returns the token: only
//! `store::resolve`, called from the checkout path, ever decrypts one.

pub mod store;

pub use store::{CredentialError, CredentialRef, NewCredential, ResolvedCredential, Scope};
