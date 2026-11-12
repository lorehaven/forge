//! Shared setup for the tests that need a real Postgres, and a way to call the API without a network.
//!
//! Skipped unless `GENEVIA_TEST_DATABASE_URL` is set. The tests make trees of their own (ids `genevia_test_*`) and delete
//! them afterwards.
//!
//! ```bash
//! docker run --rm -d --name genevia-test-pg -e POSTGRES_PASSWORD=postgres -p 55433:5432 pgvector/pgvector:pg18
//! cargo run -p foundry -- apply --catalog services/foundry/service/migrations \
//!     --database-url postgres://postgres:postgres@127.0.0.1:55433/postgres --install genevia
//! GENEVIA_TEST_DATABASE_URL=postgres://postgres:postgres@127.0.0.1:55433/postgres cargo test -p genevia
//! ```

use genevia_service::import::MAX_UPLOAD;
use genevia_service::{routers, store};
use quench_auth::domain::jwt::{Claims, JwtConfig};
use quench_core::bytes::Bytes;
use quench_db::prelude::Db;
use quench_http::di::{Container, ContainerBuilder};
use quench_http::endpoint::Endpoint;
use quench_http::extract::BodyLimit;
use quench_http::http::{HeaderMap, HeaderValue, Method, StatusCode, Uri};
use quench_http::request::Request;
use quench_serde::{Deserialize, json};
use std::sync::Arc;

/// The database the tests use, if one is set.
pub fn url() -> Option<String> {
    std::env::var("GENEVIA_TEST_DATABASE_URL")
        .ok()
        .filter(|u| !u.trim().is_empty())
}

/// Connects the store (once per process) to the test database.
pub async fn connect(url: &str) {
    store::init(Db::connect(url).await.expect("connects")).expect("store");
}

/// Calls the routes in-process.
pub struct Api {
    endpoint: Arc<dyn Endpoint>,
    container: Arc<Container>,
}

impl Api {
    /// With `enforced` the API checks the claims it is given; without, as in development, everyone may do everything.
    pub async fn new(enforced: bool) -> Self {
        routers::api::register_routes();
        let mut config = JwtConfig::for_tests();
        config.auth_enabled = enforced;
        let container = ContainerBuilder::new()
            .provide(config)
            .provide(BodyLimit(MAX_UPLOAD))
            .build()
            .await
            .expect("container");
        Self {
            endpoint: quench_starter::http::discover_and_mount("/"),
            container: Arc::new(container),
        }
    }

    /// A request as `scope` (a space-separated list of grants), or as nobody when `None`.
    pub async fn call(
        &self,
        method: Method,
        path: &str,
        scope: Option<&str>,
        body: Option<(&str, Vec<u8>)>,
    ) -> (StatusCode, String) {
        let mut headers = HeaderMap::new();
        let bytes = body.map_or_else(Bytes::new, |(content_type, bytes)| {
            headers.insert("content-type", HeaderValue::from_str(content_type).unwrap());
            Bytes::from(bytes)
        });
        let mut request = Request::new(
            method,
            path.parse::<Uri>().unwrap(),
            headers,
            quench_http::body::InboundBody::from_bytes(bytes),
            self.container.clone(),
        );
        if let Some(scope) = scope {
            request.extensions_mut().insert(Claims::for_audiences(
                "alice".to_string(),
                vec!["genevia".to_string()],
                scope.to_string(),
                None,
                3600,
            ));
        }
        let response = self.endpoint.call(request).await.into_inner();
        let status = response.status;
        let body = response.body.collect_limited(usize::MAX).await.unwrap();
        (status, String::from_utf8_lossy(&body).into_owned())
    }

    /// As everybody, in development.
    pub async fn get(&self, path: &str) -> (StatusCode, String) {
        self.call(Method::GET, path, None, None).await
    }

    /// Sends `value` as JSON and reads the answer's status.
    pub async fn send_json<B: quench_serde::Serialize>(
        &self,
        method: Method,
        path: &str,
        scope: Option<&str>,
        value: &B,
    ) -> (StatusCode, String) {
        self.call(
            method,
            path,
            scope,
            Some(("application/json", json::to_string(value).into_bytes())),
        )
        .await
    }

    /// A GEDCOM file as an upload.
    pub async fn import(
        &self,
        name: &str,
        text: &str,
        scope: Option<&str>,
    ) -> (StatusCode, String) {
        let boundary = "X-GENEVIA-TEST";
        let body = format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{name}\"\r\n\r\n{text}\r\n--{boundary}--\r\n"
        )
        .into_bytes();
        self.call(
            Method::POST,
            "/api/v1/trees",
            scope,
            Some((&format!("multipart/form-data; boundary={boundary}"), body)),
        )
        .await
    }
}

/// The answer's body as `T`.
pub fn read<T: Deserialize>(body: &str) -> T {
    json::from_str(body).unwrap_or_else(|e| panic!("{e}: {body}"))
}
