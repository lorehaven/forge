//! The log viewer, fetched when a job is opened - inlining it would open a
//! stream per job on page load, since `sse-connect` fires as soon as it exists.

use crate::routers::ui::common::{is_ui_authenticated, ui_login_redirect_for};
use async_trait::async_trait;
use quench_auth::domain::jwt::JwtConfig;
use quench_http::prelude::{
    FromRequest, HttpError, Path, Request, Response, get, http::StatusCode,
};
use quench_starter::common::routes::with_base_path;
use quench_web::prelude::*;

/// Not a plain `bool` like `PageAuth` - building the hx-aware redirect needs the request itself.
pub(super) struct LogAccess(Option<Response>);

#[async_trait]
impl FromRequest for LogAccess {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        let Ok(config) = req.container().get::<JwtConfig>() else {
            return Ok(Self(Some(ui_login_redirect_for(req))));
        };
        if is_ui_authenticated(req, &config).await {
            Ok(Self(None))
        } else {
            Ok(Self(Some(ui_login_redirect_for(req))))
        }
    }
}

/// Toolbar (open raw / copy) plus the log itself, appending each SSE frame -
/// both `sse-swap` event names land in the same place; the frame's own class keeps stderr distinguishable.
#[get("/ui/jobs/{id}/log")]
pub(super) async fn log(LogAccess(redirect): LogAccess, Path(job_id): Path<String>) -> Response {
    if let Some(resp) = redirect {
        return resp;
    }

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

    Response::html(StatusCode::OK, body.render())
}

/// `log_id` is a UUID-derived `format!("log-{job_id}")`, safe to splice unescaped.
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

pub(super) fn register_routes() {
    let _ = log as fn(_, _) -> _;
}
