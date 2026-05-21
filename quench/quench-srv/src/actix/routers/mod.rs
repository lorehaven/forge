use utoipa::OpenApi;

pub mod health;
pub mod swagger;

#[derive(OpenApi)]
#[openapi(
    nest((path = "/health", api = health::HealthApiDoc),)
)]
pub struct BaseOpenApiDoc;
