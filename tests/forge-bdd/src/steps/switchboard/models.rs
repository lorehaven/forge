use crate::world::ForgeWorld;
use cucumber::{then, when};

#[when(expr = "POST request is sent to {string} with body:")]
async fn send_post_request_with_body(
    world: &mut ForgeWorld,
    path: String,
    step: &cucumber::gherkin::Step,
) {
    let body = step.docstring().expect("Step must have a docstring");
    let url = format!("{}{}", world.switchboard_url, path);
    let mut rb = world.client.post(&url);
    rb = world.apply_auth(rb);
    let res = rb
        .header("Content-Type", "application/json")
        .body(body.to_string())
        .send()
        .await
        .expect("Failed to send POST request");
    world.record_response(res).await;
}

#[then("response should be a JSON array")]
async fn check_json_array(world: &mut ForgeWorld) {
    let json = world
        .last_json
        .as_ref()
        .expect("No JSON response available");
    assert!(json.is_array(), "Response is not a JSON array: {:?}", json);
}

#[then(expr = "all models in the response should contain {string} in their name")]
async fn check_models_name(world: &mut ForgeWorld, search: String) {
    let json = world
        .last_json
        .as_ref()
        .expect("No JSON response available");
    let models = json.as_array().expect("Response is not an array");
    let search_lower = search.to_lowercase();
    for model in models {
        let name = model["name"].as_str().expect("Model has no name");
        assert!(
            name.to_lowercase().contains(&search_lower),
            "Model name '{}' does not contain '{}'",
            name,
            search
        );
    }
}
