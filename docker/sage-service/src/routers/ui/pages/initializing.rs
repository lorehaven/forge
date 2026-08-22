use crate::clients::switchboard::{SwitchboardClient, VllmInstance};
use crate::config::{DefaultModel, SageConfig};
use crate::routers::ui::common;
use crate::routers::ui::common::{UiPageKind, render_page};
use actix_web::{HttpResponse, Responder, get, web};
use quench_auth::prelude::JwtConfig;
use quench_starter::prelude::with_base_path;
use quench_web::prelude::*;

/// Launch state of a configured default model; the initializing screen blocks until every
/// model reaches `Running`, since all of them are required for Sage to function.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ModelState {
    Running,
    /// Process launched, model still loading weights / warming up.
    Starting,
    /// Not launched yet (monitor task will request a launch shortly).
    Pending,
    Failed,
    /// Switchboard could not be reached, so the real state is unknown.
    Unknown,
}

/// Resolve a model's state from switchboard's instances; if more than one matches (e.g. a
/// stale failed one alongside a fresh starting one), the healthiest state wins.
pub fn model_state(model: &DefaultModel, instances: &[VllmInstance]) -> ModelState {
    let mut state = ModelState::Pending;
    for inst in instances.iter().filter(|i| i.model == model.name) {
        let candidate = match inst.status.as_str() {
            "running" => ModelState::Running,
            "starting" | "pending" => ModelState::Starting,
            "failed" => ModelState::Failed,
            _ => continue, // ignore "terminating" and anything unexpected
        };
        // Running beats Starting beats Failed beats Pending.
        let rank = |s: ModelState| match s {
            ModelState::Running => 3,
            ModelState::Starting => 2,
            ModelState::Failed => 1,
            _ => 0,
        };
        if rank(candidate) > rank(state) {
            state = candidate;
        }
    }
    state
}

/// True when every configured default model has a running instance; gates the home page and the initializing screen.
pub fn all_models_running(defaults: &[DefaultModel], instances: &[VllmInstance]) -> bool {
    defaults
        .iter()
        .all(|m| model_state(m, instances) == ModelState::Running)
}

/// Human-friendly label for a model, tagging embedding models so it's obvious why a non-chat model is in the list.
pub fn model_label(model: &DefaultModel) -> Element {
    let name = span().text(&model.name);
    match model.task.as_deref() {
        Some("embed") | Some("embedding") => span().child(name).child(
            span()
                .attr("data-i18n", "ui_init_embedding_tag")
                .text(" (embedding)"),
        ),
        _ => name,
    }
}

/// (icon class, status text, i18n key, css modifier) for a model state.
pub fn state_presentation(
    state: ModelState,
) -> (&'static str, &'static str, &'static str, &'static str) {
    match state {
        ModelState::Running => (
            "fas fa-check-circle",
            "Running",
            "ui_init_status_running",
            "running",
        ),
        ModelState::Starting => (
            "fas fa-circle-notch fa-spin",
            "Starting…",
            "ui_init_status_starting",
            "starting",
        ),
        ModelState::Pending => ("fas fa-clock", "Queued", "ui_init_status_queued", "pending"),
        ModelState::Failed => (
            "fas fa-times-circle",
            "Failed",
            "ui_init_status_failed",
            "failed",
        ),
        ModelState::Unknown => (
            "fas fa-plug",
            "Connecting…",
            "ui_init_status_unknown",
            "unknown",
        ),
    }
}

/// Render the model status rows; shared between the full-page render and the polling fragment so markup stays identical.
pub fn render_model_rows(
    defaults: &[DefaultModel],
    instances_res: &anyhow::Result<Vec<VllmInstance>>,
) -> Element {
    let mut rows = div().class("model-rows");

    for model in defaults {
        let state = match instances_res {
            Ok(instances) => model_state(model, instances),
            Err(_) => ModelState::Unknown,
        };
        let (icon, label_text, label_key, modifier) = state_presentation(state);

        rows = rows.child(
            div()
                .class(format!("model-row model-row--{}", modifier))
                .child(
                    div()
                        .class("model-row-name")
                        .child(i().class("fas fa-microchip model-row-icon"))
                        .child(model_label(model)),
                )
                .child(
                    div()
                        .class(format!("model-status model-status--{}", modifier))
                        .child(i().class(icon))
                        .child(span().attr("data-i18n", label_key).text(label_text)),
                ),
        );
    }

    rows
}

pub fn render_initializing_page(
    defaults: &[DefaultModel],
    instances_res: &anyhow::Result<Vec<VllmInstance>>,
) -> HttpResponse {
    let switchboard_down = instances_res.is_err();

    let card = div()
        .class("init-card")
        .child(i().class("fas fa-circle-notch fa-spin init-spinner"))
        .child(
            h2().class("init-title")
                .attr("data-i18n", "ui_init_title")
                .text("Preparing Sage"),
        )
        .child(
            div()
                .class("init-subtitle")
                .attr("data-i18n", "ui_init_subtitle")
                .text("Launching the models Sage needs before you can start chatting."),
        )
        // Poll the status fragment; it swaps these rows and issues an HX-Redirect to /ui/home once every model is running.
        .child(
            div()
                .attr("id", "model-status-list")
                .attr("hx-get", with_base_path("/ui/initializing/status"))
                .attr("hx-trigger", "every 3s")
                .attr("hx-swap", "innerHTML")
                .child(render_model_rows(defaults, instances_res)),
        )
        .child_opt(switchboard_down.then(|| {
            div()
                .class("init-warning")
                .child(i().class("fas fa-exclamation-triangle"))
                .child(
                    span()
                        .attr("data-i18n", "ui_init_waiting")
                        .text("Waiting for the model service to respond…"),
                )
        }));

    render_page(
        HttpResponse::Ok(),
        content().class("init-content").child(card),
        UiPageKind::Home,
    )
}

#[get("/initializing")]
pub(super) async fn initializing(
    req: actix_web::HttpRequest,
    jwt_config: web::Data<JwtConfig>,
    switchboard: web::Data<SwitchboardClient>,
    sage_config: web::Data<SageConfig>,
) -> impl Responder {
    if !common::is_ui_authenticated(&req, &jwt_config).await {
        return common::ui_login_redirect();
    }

    let instances = switchboard.get_vllm_instances().await;
    if let Ok(ref insts) = instances
        && all_models_running(&sage_config.default_models, insts)
    {
        return HttpResponse::Found()
            .append_header(("Location", with_base_path("/ui/home")))
            .finish();
    }

    render_initializing_page(&sage_config.default_models, &instances)
}

#[get("/initializing/status")]
pub(super) async fn initializing_status(
    req: actix_web::HttpRequest,
    jwt_config: web::Data<JwtConfig>,
    switchboard: web::Data<SwitchboardClient>,
    sage_config: web::Data<SageConfig>,
) -> impl Responder {
    if !common::is_ui_authenticated(&req, &jwt_config).await {
        return HttpResponse::Unauthorized().finish();
    }

    let instances = switchboard.get_vllm_instances().await;
    if let Ok(ref insts) = instances
        && all_models_running(&sage_config.default_models, insts)
    {
        // Every model is up — tell htmx to navigate to the real home screen.
        return HttpResponse::Ok()
            .append_header(("HX-Redirect", with_base_path("/ui/home")))
            .finish();
    }

    HttpResponse::Ok()
        .content_type("text/html")
        .body(render_model_rows(&sage_config.default_models, &instances).render())
}
