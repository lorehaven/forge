//! Steps for `/admin` garbage collection. `routers::admin::wrap_auth` mounts `Auth` +
//! `RequireWrite` over the prefix; `I hold a token scoped {string}` / `I hold no token` are
//! shared with the files steps (`src/steps/warehouse/files.rs`).

use crate::world::ForgeWorld;
use quench_test_bdd::when;

async fn run_gc(world: &mut ForgeWorld, kind: &str) {
    let url = format!("{}/admin/{kind}/gc", world.warehouse_url);
    let mut builder = world.client.post(&url);
    if let Some(token) = &world.token {
        builder = builder.bearer_auth(token);
    }
    let res = builder.send().await.expect("admin GC request failed");
    world.record_response(res).await;
}

#[when("I run crates garbage collection")]
async fn crates_gc(world: &mut ForgeWorld) {
    run_gc(world, "crates").await;
}

#[when("I run docker garbage collection")]
async fn docker_gc(world: &mut ForgeWorld) {
    run_gc(world, "docker").await;
}
