//! Gauge scenarios use real Gatehouse-signed tokens, an in-memory history and a mock `node_exporter`
//! (`mocks::start_node_exporter`) as the one host that answers.
use crate::world::ForgeWorld;
use quench_test_bdd::{given, when};

fn no_redirect_client() -> quench_client::transport::Client {
    quench_client::transport::Client::builder()
        .danger_accept_invalid_certs(true)
        .redirect(quench_client::transport::Redirect::None)
        .build()
        .expect("http client")
}

#[given("gauge API is available")]
const fn available(world: &mut ForgeWorld) {
    world.target = crate::world::Target::Gauge;
}

#[given(expr = "I am authenticated against gauge with scope {string}")]
async fn authenticated_with_scope(world: &mut ForgeWorld, scope: String) {
    world.access_token = Some(
        crate::world::mint_test_token(
            &world.client,
            &world.gatehouse_url,
            "gauge-bdd-user",
            &["gauge"],
            &scope,
        )
        .await,
    );
}

/// Follows nothing, so a redirect can be asserted on rather than chased.
#[when(expr = "I open the gauge path {string}")]
async fn open_path(world: &mut ForgeWorld, path: String) {
    let url = format!("{}{path}", world.gauge_url);
    let response = no_redirect_client()
        .get(&url)
        .send()
        .await
        .unwrap_or_else(|e| panic!("GET {url}: {e}"));
    world.record_response(response).await;
}
