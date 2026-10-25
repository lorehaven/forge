//! Black-box security scanner for the forge services.
//!
//! Point it at a running estate (a local compose stack, the BDD harness, staging) and it enumerates
//! every declared route, then attacks the *behaviour* of each service from outside: who can reach
//! what without credentials, whether forged tokens work, whether paths escape, whether errors leak,
//! whether the HTTP parser can be desynchronised.
//!
//! It is non-destructive by default (read-only probes, sentinel identifiers that cannot name real
//! data, no flooding) and refuses non-private targets unless told they are yours.

pub mod census;
pub mod checks;
pub mod finding;
pub mod gate;
pub mod scan;
pub mod wire;
