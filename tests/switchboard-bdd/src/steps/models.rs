use crate::steps::common::SwitchboardWorld;
use cucumber::{then, when};

#[when(expr = "POST request is sent to {string} with body:")]
async fn send_post_request_with_body(
    world: &mut SwitchboardWorld,
    path: String,
    step: &cucumber::gherkin::Step,
) {
    let body = step.docstring().expect("Step must have a docstring");
    let url = format!("{}{}", world.api_url, path);
    let res = world
        .client
        .post(&url)
        .header("Content-Type", "application/json")
        .body(body.to_string())
        .send()
        .await
        .expect("Failed to send POST request");
    world.record_response(res).await;
}

#[then("response should be a JSON array")]
async fn check_json_array(world: &mut SwitchboardWorld) {
    let json = world
        .last_json
        .as_ref()
        .expect("No JSON response available");
    assert!(json.is_array(), "Response is not a JSON array: {:?}", json);
}

#[then(expr = "all models in the response should contain {string} in their name")]
async fn check_models_name(world: &mut SwitchboardWorld, search: String) {
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

#[then("response should be a JSON object")]
async fn check_json_object(world: &mut SwitchboardWorld) {
    let json = world
        .last_json
        .as_ref()
        .expect("No JSON response available");
    assert!(
        json.is_object(),
        "Response is not a JSON object: {:?}",
        json
    );
}

#[then(expr = "response content type should be {string}")]
async fn check_content_type(world: &mut SwitchboardWorld, expected: String) {
    let actual = world
        .last_response_headers
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|h| h.to_str().ok())
        .expect("No Content-Type header");
    assert!(
        actual.contains(&expected),
        "Content-Type '{}' does not contain '{}'",
        actual,
        expected
    );
}
