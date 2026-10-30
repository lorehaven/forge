//! Git authentication conveyor holds per repository/project. Sibling to [`crate::secrets`], not a
//! variant: resolved automatically by the checkout, never named by a pipeline; sealed under its own key.

pub mod store;

pub use store::{CredentialError, CredentialRef, NewCredential, ResolvedCredential, Scope};
