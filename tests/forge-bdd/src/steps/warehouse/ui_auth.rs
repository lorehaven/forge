//! Login and logout are gatehouse's; this service only redirects to it.

use crate::world::ForgeWorld;
use cucumber::when;

fn no_redirect_client() -> reqwest::Client {
    reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("http client")
}

#[when("I open the login page")]
async fn open_login(world: &mut ForgeWorld) {
    let url = format!("{}/ui/login", world.target_url());
    let res = no_redirect_client()
        .get(&url)
        .send()
        .await
        .expect("login request failed");
    world.record_response(res).await;
}

#[when("I open the logout page")]
async fn open_logout(world: &mut ForgeWorld) {
    let url = format!("{}/ui/logout", world.target_url());
    let res = no_redirect_client()
        .get(&url)
        .send()
        .await
        .expect("logout request failed");
    world.record_response(res).await;
}
