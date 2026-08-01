//! The one place switchboard's per-action grants (`launch`, `stop`,
//! `delete-model`) are actually exercised: each route checks `mod_impl::can`
//! for its own action, not a coarse "write", so a token scoped for one must
//! be refused by the others.

use crate::world::{ForgeWorld, mint_test_token};
use cucumber::given;

#[given(expr = "I hold a switchboard token scoped {string}")]
async fn hold_switchboard_token(world: &mut ForgeWorld, scope: String) {
    world.switchboard_token =
        mint_test_token(&world.client, &world.gatehouse_url, "bdd-user", &["switchboard"], &scope).await;
}
