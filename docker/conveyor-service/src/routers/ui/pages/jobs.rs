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

/// The element htmx swaps into the opened job: a toolbar above the log
/// itself, so a person mid-incident can get the raw text out of conveyor -
/// into another tab, or onto their clipboard for wherever they are pasting a
/// build failure - without scraping it out of the rendered `<span>`s by hand.
///
/// The log connects to the stream and appends each frame to itself. Both
/// `sse-swap` event names land in the same place; the frame carries its own
/// class, so stderr is still distinguishable.
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
    let log_id = format!("log-{job_id}");
    let stream = with_base_path(&format!("/api/v1/jobs/{job_id}/stream?format=html"));
    let raw = with_base_path(&format!("/api/v1/jobs/{job_id}/raw"));

    let toolbar = div()
        .class("log-toolbar")
        .child(
            a().class("log-action")
                .attr("href", raw)
                .attr("target", "_blank")
                .attr("rel", "noopener")
                .attr("title", "Open raw log")
                .attr("data-i18n-title", "ui_log_raw_tooltip")
                .child(i().class("fas").class("fa-up-right-from-square")),
        )
        .child(
            button()
                .attr("type", "button")
                .class("log-action")
                .attr("title", "Copy log")
                .attr("data-i18n-title", "ui_log_copy_tooltip")
                .attr("onclick", copy_log_js(&log_id))
                .child(i().class("fas").class("fa-copy")),
        );

    let viewer = div()
        .class("log")
        .attr("id", &log_id)
        .attr("hx-ext", "sse")
        .attr("sse-connect", stream)
        .attr("sse-swap", "stdout,stderr")
        .attr("hx-swap", "beforeend");

    let body = div().child(toolbar).child(viewer);

    HttpResponse::Ok()
        .content_type(ContentType::html())
        .body(body.render())
}

/// `log_id` is this handler's own `format!("log-{job_id}")`, made of a UUID -
/// safe to splice into a JS string literal with no escaping of its own.
fn copy_log_js(log_id: &str) -> String {
    format!(
        "const icon = this.querySelector('i'); \
         navigator.clipboard.writeText(document.getElementById('{log_id}').innerText) \
         .then(() => {{ \
             const cls = icon.className; \
             icon.className = 'fas fa-check'; \
             setTimeout(() => {{ icon.className = cls; }}, 1500); \
         }}) \
         .catch(err => console.error('could not copy the log', err));"
    )
}
