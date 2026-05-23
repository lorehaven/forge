use utoipa::OpenApi;

pub mod health;
pub mod swagger;
pub mod ui;

#[derive(OpenApi)]
#[openapi(
    nest((path = "/health", api = health::HealthApiDoc),)
)]
pub struct BaseOpenApiDoc;
