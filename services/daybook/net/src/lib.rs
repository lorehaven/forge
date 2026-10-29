//! Daybook's talk to the services: gatehouse for the session (the shared `gatehouse-client`), the daybook service
//! for the tasks. Plain blocking calls (run them on a worker thread).
//!
//! Tokens: the password is used once, by [`login`]; the refresh token rotates, so it is handed to the caller to keep
//! **before** the next request (see [`Session::keeping_tokens`] and [`Session::following`]).

pub mod api;
pub mod store;

pub use store::{Settings, Store};

use std::fmt;

#[cfg(feature = "client")]
mod client;
#[cfg(feature = "client")]
pub use client::{Session, login, set_timeouts};

/// What went wrong, in the terms a screen shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetError {
    /// The services could not be reached (or answered something unreadable).
    Network(String),
    /// The user name or password is wrong, or the refresh token is no longer good.
    Unauthorized,
    /// The account has a second factor, which daybook cannot ask for: turn it off for this account.
    MfaRequired,
    /// Any other answer from the server.
    Status(u16),
    /// Too many wrong passwords: gatehouse refuses this account for a while, even with the right one.
    AccountLocked,
    /// An administrator turned this account off.
    AccountDisabled,
    /// The account's email address was never confirmed.
    EmailUnverified,
    /// The account may not use daybook (it needs a grant in gatehouse).
    Forbidden,
    /// The task, list or label is gone.
    NotFound,
    /// The service refused what was sent (an empty name, a repeat with no due date).
    Invalid,
    /// A label with that name exists.
    NameTaken,
}

impl fmt::Display for NetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Network(why) => write!(f, "cannot reach the server: {why}"),
            Self::Unauthorized => f.write_str("the user name or password is wrong, or the session ended"),
            Self::MfaRequired => f.write_str("this account needs a second factor: turn MFA off for it to use daybook"),
            Self::Status(code) => write!(f, "the server answered {code}"),
            Self::AccountLocked => f.write_str(
                "this account is locked after too many wrong passwords: try again in about 15 minutes, or have it unlocked in gatehouse",
            ),
            Self::AccountDisabled => f.write_str("this account is disabled"),
            Self::EmailUnverified => f.write_str("confirm the email address of this account first (gatehouse sent a link)"),
            Self::Forbidden => f.write_str("this account is not allowed to use daybook: ask for access in gatehouse"),
            Self::NotFound => f.write_str("it is gone: refresh to see what is left"),
            Self::Invalid => f.write_str("the server did not accept that"),
            Self::NameTaken => f.write_str("that name is already in use"),
        }
    }
}

impl std::error::Error for NetError {}

/// A status as the result a caller can use: `Ok` for a success, the error a screen can say otherwise.
///
/// # Errors
/// [`NetError`] for any status that is not a success.
pub fn check_status(status: u16) -> Result<(), NetError> {
    match status {
        200..=299 => Ok(()),
        400 => Err(NetError::Invalid),
        401 => Err(NetError::Unauthorized),
        403 => Err(NetError::Forbidden),
        404 => Err(NetError::NotFound),
        409 => Err(NetError::NameTaken),
        other => Err(NetError::Status(other)),
    }
}
