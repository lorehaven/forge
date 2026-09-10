//! Sage's UI-auth delegation reuses the shared `I open the login page` /
//! `I open the logout page` and `... protected page ...` steps
//! (`src/steps/warehouse/ui_auth.rs`, `src/steps/warehouse/ui_jwt.rs`), which
//! address whichever service the `Given sage API is available` background
//! selected. Only the scoped-token chat GET is sage-specific enough to live
//! here.

use crate::world::{ForgeWorld, mint_test_token};
use cucumber::when;

/// GETs a `/api/v1/chat/*` path with a freshly minted sage token of the given
/// scope - how the suite proves the read endpoints under that scope are gated
/// by `RequireWrite` exactly as the write ones are.
#[when(expr = "GET {string} is sent with a sage token scoped {string}")]
async fn sage_get_with_scope(world: &mut ForgeWorld, path: String, scope: String) {
    let token = mint_test_token(
        &world.client,
        &world.gatehouse_url,
        "bdd-sage",
        &["sage"],
        &scope,
    )
    .await;
    let url = format!("{}{path}", world.sage_url);
    let res = world
        .client
        .get(&url)
        .bearer_auth(token)
        .send()
        .await
        .expect("sage chat GET failed");
    world.record_response(res).await;
}

/// A well-formed, JWKS-verifiable token whose audience is some other service -
/// gatehouse's audience narrowing is what makes it invalid here, before any
/// permission is even considered.
#[when(expr = "GET {string} is sent with a token for another service")]
async fn sage_get_wrong_audience(world: &mut ForgeWorld, path: String) {
    let token = mint_test_token(
        &world.client,
        &world.gatehouse_url,
        "bdd-sage",
        &["switchboard"],
        "user switchboard:read",
    )
    .await;
    let url = format!("{}{path}", world.sage_url);
    let res = world
        .client
        .get(&url)
        .bearer_auth(token)
        .send()
        .await
        .expect("sage chat GET failed");
    world.record_response(res).await;
}
