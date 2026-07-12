use crate::steps::common::SageWorld;
use reqwest::multipart::{Form, Part};

fn file_part(file_name: &str) -> Part {
    let content: &[u8] = match file_name.rsplit('.').next() {
        Some("md") => b"# Test Document\n\nSome markdown content for testing.\n",
        Some("csv") => b"name,value\nalpha,1\nbeta,2\n",
        Some("txt") => b"Plain text content for testing.\n",
        _ => b"binary-ish content",
    };
    Part::bytes(content.to_vec()).file_name(file_name.to_string())
}

async fn record_response(
    world: &mut SageWorld,
    response: Result<reqwest::Response, reqwest::Error>,
) {
    match response {
        Ok(resp) => {
            world.last_status = Some(resp.status().as_u16());
            world.last_body = resp.text().await.ok();
        }
        Err(e) => {
            world.last_status = None;
            world.last_body = Some(e.to_string());
        }
    }
}

#[cucumber::when("I request the file list without authentication")]
async fn list_files_unauthenticated(world: &mut SageWorld) {
    let url = format!("{}/sage/api/v1/files?project_id=any", world.api_base_url);
    let response = world.client.get(&url).send().await;
    record_response(world, response).await;
}

#[cucumber::when("I request the file list without a scope")]
async fn list_files_without_scope(world: &mut SageWorld) {
    let url = format!("{}/sage/api/v1/files", world.api_base_url);
    let response = world
        .client
        .get(&url)
        .bearer_auth(&world.jwt_token)
        .send()
        .await;
    record_response(world, response).await;
}

#[cucumber::when(expr = "I request the file list for conversation {string}")]
async fn list_files_for_conversation(world: &mut SageWorld, conversation_id: String) {
    let url = format!(
        "{}/sage/api/v1/files?conversation_id={}",
        world.api_base_url, conversation_id
    );
    let response = world
        .client
        .get(&url)
        .bearer_auth(&world.jwt_token)
        .send()
        .await;
    record_response(world, response).await;
}

#[cucumber::when(expr = "I request the file list for project {string}")]
async fn list_files_for_project(world: &mut SageWorld, project_id: String) {
    let url = format!(
        "{}/sage/api/v1/files?project_id={}",
        world.api_base_url, project_id
    );
    let response = world
        .client
        .get(&url)
        .bearer_auth(&world.jwt_token)
        .send()
        .await;
    record_response(world, response).await;
}

#[cucumber::when(expr = "I upload file {string} to project {string}")]
async fn upload_file_to_project(world: &mut SageWorld, file_name: String, project_id: String) {
    let url = format!("{}/sage/api/v1/files", world.api_base_url);
    let form = Form::new()
        .part("file", file_part(&file_name))
        .text("project_id", project_id);
    let response = world
        .client
        .post(&url)
        .bearer_auth(&world.jwt_token)
        .multipart(form)
        .send()
        .await;
    record_response(world, response).await;
}

#[cucumber::when(expr = "I upload file {string} to conversation {string}")]
async fn upload_file_to_conversation(
    world: &mut SageWorld,
    file_name: String,
    conversation_id: String,
) {
    let url = format!("{}/sage/api/v1/files", world.api_base_url);
    let form = Form::new()
        .part("file", file_part(&file_name))
        .text("conversation_id", conversation_id);
    let response = world
        .client
        .post(&url)
        .bearer_auth(&world.jwt_token)
        .multipart(form)
        .send()
        .await;
    record_response(world, response).await;
}

#[cucumber::when(expr = "I upload file {string} without a target scope")]
async fn upload_file_without_scope(world: &mut SageWorld, file_name: String) {
    let url = format!("{}/sage/api/v1/files", world.api_base_url);
    let form = Form::new().part("file", file_part(&file_name));
    let response = world
        .client
        .post(&url)
        .bearer_auth(&world.jwt_token)
        .multipart(form)
        .send()
        .await;
    record_response(world, response).await;
}

#[cucumber::when(expr = "I upload file {string} to project {string} without authentication")]
async fn upload_file_unauthenticated(world: &mut SageWorld, file_name: String, project_id: String) {
    let url = format!("{}/sage/api/v1/files", world.api_base_url);
    let form = Form::new()
        .part("file", file_part(&file_name))
        .text("project_id", project_id);
    let response = world.client.post(&url).multipart(form).send().await;
    record_response(world, response).await;
}

#[cucumber::when(expr = "I request metadata for file {string}")]
async fn get_file_metadata(world: &mut SageWorld, file_id: String) {
    let url = format!("{}/sage/api/v1/files/{}", world.api_base_url, file_id);
    let response = world
        .client
        .get(&url)
        .bearer_auth(&world.jwt_token)
        .send()
        .await;
    record_response(world, response).await;
}

#[cucumber::when(expr = "I delete file {string}")]
async fn delete_file(world: &mut SageWorld, file_id: String) {
    let url = format!("{}/sage/api/v1/files/{}", world.api_base_url, file_id);
    let response = world
        .client
        .delete(&url)
        .bearer_auth(&world.jwt_token)
        .send()
        .await;
    record_response(world, response).await;
}

#[cucumber::when(expr = "I reprocess file {string}")]
async fn reprocess_file(world: &mut SageWorld, file_id: String) {
    let url = format!(
        "{}/sage/api/v1/files/{}/reprocess",
        world.api_base_url, file_id
    );
    let response = world
        .client
        .post(&url)
        .bearer_auth(&world.jwt_token)
        .send()
        .await;
    record_response(world, response).await;
}

#[cucumber::when(expr = "I download file {string}")]
async fn download_file(world: &mut SageWorld, file_id: String) {
    let url = format!(
        "{}/sage/api/v1/files/{}/download",
        world.api_base_url, file_id
    );
    let response = world
        .client
        .get(&url)
        .bearer_auth(&world.jwt_token)
        .send()
        .await;
    record_response(world, response).await;
}

#[cucumber::when(expr = "I download file {string} without authentication")]
async fn download_file_unauthenticated(world: &mut SageWorld, file_id: String) {
    let url = format!(
        "{}/sage/api/v1/files/{}/download",
        world.api_base_url, file_id
    );
    let response = world.client.get(&url).send().await;
    record_response(world, response).await;
}

#[cucumber::when(expr = "I request chunks for file {string}")]
async fn request_chunks(world: &mut SageWorld, file_id: String) {
    let url = format!(
        "{}/sage/api/v1/files/{}/chunks",
        world.api_base_url, file_id
    );
    let response = world
        .client
        .get(&url)
        .bearer_auth(&world.jwt_token)
        .send()
        .await;
    record_response(world, response).await;
}

#[cucumber::when("I request the chat capabilities")]
async fn get_capabilities(world: &mut SageWorld) {
    let url = format!("{}/sage/api/v1/chat/capabilities", world.api_base_url);
    let response = world
        .client
        .get(&url)
        .bearer_auth(&world.jwt_token)
        .send()
        .await;
    record_response(world, response).await;
}

#[cucumber::when("I request the files panel without authentication")]
async fn files_panel_unauthenticated(world: &mut SageWorld) {
    let url = format!("{}/sage/ui/files/panel?project_id=any", world.api_base_url);
    let response = world.client.get(&url).send().await;
    record_response(world, response).await;
}

#[cucumber::then(expr = "the response should mention {string}")]
async fn response_should_mention(world: &mut SageWorld, needle: String) -> Result<(), String> {
    match &world.last_body {
        Some(body) if body.contains(&needle) => Ok(()),
        Some(body) => Err(format!(
            "Expected response to contain '{}', got: {}",
            needle,
            &body[..body.len().min(300)]
        )),
        None => Err("No response body recorded".to_string()),
    }
}
