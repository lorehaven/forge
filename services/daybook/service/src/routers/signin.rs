//! Sign-in for the browser client, through this service.
//!
//! The page's own server answers `/auth/login` and `/auth/refresh` and passes them on to gatehouse (server to server). The
//! browser then never needs gatehouse on its own address (a page may only call the server it came from, and in a local
//! estate gatehouse is on another port); in a deployment that shares one address it is the same call as before. The answers
//! are gatehouse's, status and body as they are.

use quench_http::prelude::http::StatusCode;
use quench_http::prelude::{Json, Response, post};
use quench_serde::Value;

/// Where gatehouse's `path` is: under `GATEHOUSE_URL`, which names the realm (`https://host/gatehouse`).
#[must_use]
pub fn gatehouse_target(base: &str, path: &str) -> String {
    format!("{}/api/v1/auth/{path}", base.trim().trim_end_matches('/'))
}

/// Forwards `body` to gatehouse's `path` and gives back what it said. Certificates are checked unless
/// `GATEHOUSE_TLS_VERIFY` says not to, as for every other call to gatehouse (a local estate has a self-signed one).
async fn forward(path: &'static str, body: Value) -> Response {
    let url = gatehouse_target(&quench_core::env::get_or("GATEHOUSE_URL", ""), path);
    let verify: bool = quench_core::env::get_or("GATEHOUSE_TLS_VERIFY", "true")
        .parse()
        .unwrap_or(true);
    let answered = async {
        let client = quench_client::transport::Client::builder()
            .danger_accept_invalid_certs(!verify)
            .build()
            .map_err(|e| e.to_string())?;
        let response = client
            .post(url)
            .json(&body)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let status = response.status().as_u16();
        let text = response.text().await.map_err(|e| e.to_string())?;
        Ok::<_, String>((status, text))
    }
    .await;
    match answered {
        Ok((status, text)) => Response::from_bytes(
            StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY),
            text.into_bytes().into(),
        )
        .header("content-type", "application/json")
        .header("cache-control", "no-store"),
        Err(why) => {
            quench_log::warn!("sign-in: cannot reach gatehouse: {why}");
            Response::from_bytes(
                StatusCode::BAD_GATEWAY,
                b"{\"error\":\"cannot reach the sign-in service\"}"
                    .to_vec()
                    .into(),
            )
            .header("content-type", "application/json")
        }
    }
}

#[post("/auth/login")]
pub async fn login(Json(body): Json<Value>) -> Response {
    forward("login", body).await
}

#[post("/auth/refresh")]
pub async fn refresh(Json(body): Json<Value>) -> Response {
    forward("refresh", body).await
}

pub fn register_routes() {
    let _ = login as fn(_) -> _;
    let _ = refresh as fn(_) -> _;
}
