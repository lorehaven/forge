//! The one place switchboard's per-action grants (`launch`, `stop`,
//! `delete-model`) are actually exercised: each route checks `mod_impl::can`
//! for its own action, not a coarse "write", so a token scoped for one must
//! be refused by the others.

use crate::world::{ForgeWorld, build_test_jwt};
use cucumber::given;

/// The suite shares one signing key across every service (`services.rs`), the
/// same variable `JwtConfig::init` reads, so this is the same secret
/// switchboard is verifying against.
fn jwt_secret() -> String {
    std::env::var("JWT_SECRET").unwrap_or_else(|_| "forge-bdd-shared-secret".to_string())
}

#[given(expr = "I hold a switchboard token scoped {string}")]
async fn hold_switchboard_token(world: &mut ForgeWorld, scope: String) {
    world.switchboard_token =
        build_test_jwt("bdd-user", "switchboard", &scope, &jwt_secret(), 3600);
}
