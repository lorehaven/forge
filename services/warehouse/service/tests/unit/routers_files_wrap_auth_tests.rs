//! `files::wrap_auth` puts the verified identity on the request for every route that reads it: the files and the index of
//! the dynamic storages. A route outside the wrapped prefixes sees no identity at all, which is how `GET /api/v1/storages`
//! once answered 403 to everyone.

use quench_auth::domain::jwt::{Claims, JwtConfig};
use quench_core::bytes::Bytes;
use quench_http::body::InboundBody;
use quench_http::di::{Container, ContainerBuilder};
use quench_http::endpoint::Endpoint;
use quench_http::http::{HeaderMap, Method, StatusCode, Uri};
use quench_http::request::Request;
use quench_http::response::Response;
use std::sync::Arc;
use warehouse_service::routers::files;

fn config() -> JwtConfig {
    let mut config = JwtConfig::for_tests_with_signing();
    config.service_name = "warehouse".to_string();
    config.audiences = vec!["warehouse".to_string()];
    config.auth_enabled = true;
    config
}

/// 200 when the request carries an identity that holds the blanket `warehouse:read`, 403 when it carries none or another.
struct NeedsRead;

#[quench_core::async_trait]
impl Endpoint for NeedsRead {
    async fn call(&self, req: Request) -> Response {
        let allowed = req
            .extensions()
            .get::<Claims>()
            .is_some_and(|claims| claims.can("warehouse", "read"));
        Response::new(if allowed {
            StatusCode::OK
        } else {
            StatusCode::FORBIDDEN
        })
    }
}

async fn call(config: &JwtConfig, path: &str, token: Option<&str>) -> StatusCode {
    let app = files::wrap_auth(Arc::new(NeedsRead), config.clone(), "/warehouse");
    let container: Arc<Container> = Arc::new(ContainerBuilder::new().build().await.unwrap());
    let mut headers = HeaderMap::new();
    if let Some(token) = token {
        headers.insert("authorization", format!("Bearer {token}").parse().unwrap());
    }
    let req = Request::new(
        Method::GET,
        path.parse::<Uri>().unwrap(),
        headers,
        InboundBody::from_bytes(Bytes::new()),
        container,
    );
    app.call(req).await.status()
}

async fn reader(config: &JwtConfig) -> String {
    // What gatehouse's permission editor sends: a service identity with the one read grant, audience this service.
    config
        .issue_access_token_for(
            "gatehouse".to_string(),
            vec!["warehouse".to_string()],
            "user warehouse:read".to_string(),
            None,
        )
        .await
        .unwrap()
}

#[quench_test::test]
async fn the_files_and_the_storage_index_both_see_the_callers_identity() {
    // One config for the token and the check: each `config()` makes its own signing key.
    let config = config();
    let token = reader(&config).await;
    assert_eq!(
        call(&config, "/warehouse/api/v1/files", Some(&token)).await,
        StatusCode::OK
    );
    assert_eq!(
        call(&config, "/warehouse/api/v1/storages", Some(&token)).await,
        StatusCode::OK
    );
}

#[quench_test::test]
async fn without_a_token_the_storage_index_is_unauthorized_not_open() {
    assert_eq!(
        call(&config(), "/warehouse/api/v1/storages", None).await,
        StatusCode::UNAUTHORIZED
    );
}

#[quench_test::test]
async fn a_route_outside_the_wrapped_prefixes_gets_no_identity() {
    // This is how `GET /api/v1/storages` answered 403 to everyone before it was mounted.
    let config = config();
    let token = reader(&config).await;
    assert_eq!(
        call(&config, "/warehouse/api/v1/elsewhere", Some(&token)).await,
        StatusCode::FORBIDDEN
    );
}
