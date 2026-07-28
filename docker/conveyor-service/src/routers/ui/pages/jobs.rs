//! The log viewer, fetched when a job is opened.
//!
//! Returned as a fragment rather than rendered with the run page, because
//! `sse-connect` opens its connection as soon as the element exists. Inlining
//! it would mean a run with eight jobs opening eight streams on page load and
//! holding eight workers, whether or not anyone looked at them.

use crate::routers::ui::common::{is_ui_authenticated, ui_login_redirect_for};
use actix_web::{HttpRequest, HttpResponse, Responder, get, http::header::ContentType, web};
use quench_auth::prelude::JwtConfig;
use quench_starter::prelude::with_base_path;
use quench_web::prelude::*;

/// The element htmx swaps into the opened job.
///
/// It connects to the stream and appends each frame to itself. Both `sse-swap`
/// event names land in the same place; the frame carries its own class, so
/// stderr is still distinguishable.
#[get("/jobs/{id}/log")]
pub(super) async fn log(
    request: HttpRequest,
    path: web::Path<String>,
    config: web::Data<JwtConfig>,
) -> impl Responder {
    if !is_ui_authenticated(&request, &config).await {
        return ui_login_redirect_for(&request);
    }

    let job_id = path.into_inner();
    let stream = with_base_path(&format!("/api/v1/jobs/{job_id}/stream?format=html"));

    let viewer = div()
        .class("log")
        .attr("id", format!("log-{job_id}"))
        .attr("hx-ext", "sse")
        .attr("sse-connect", stream)
        .attr("sse-swap", "stdout,stderr")
        .attr("hx-swap", "beforeend");

    HttpResponse::Ok()
        .content_type(ContentType::html())
        .body(viewer.render())
}
