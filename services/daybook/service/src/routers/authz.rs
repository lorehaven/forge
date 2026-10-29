//! Per-operation access on top of the blanket `daybook:<action>` grant.
//!
//! Rows are always scoped to the caller's own `sub`, so there is no resource
//! scoping - only whether the caller may read or may write at all.

use quench_auth::domain::jwt::{Claims, JwtConfig};

/// Whether the caller holds `daybook:<action>`. `auth_enabled` off is the
/// realm-wide dev bypass.
#[must_use]
pub fn can(claims: Option<&Claims>, config: &JwtConfig, action: &str) -> bool {
    if !config.auth_enabled {
        return true;
    }

    claims.is_some_and(|claims| claims.can("daybook", action))
}
