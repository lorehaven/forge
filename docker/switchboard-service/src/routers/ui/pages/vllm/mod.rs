use crate::routers::models::mod_impl::{OptionalClaims, can};
use crate::routers::ui::UiAuthenticated;
use crate::routers::ui::common::{UiPageKind, render_page};
use quench_auth::domain::jwt::JwtConfig;
use quench_http::prelude::{Inject, Response, get};
use quench_starter::common::routes::with_base_path;
use quench_web::prelude::*;

#[get("/ui/vllm/manage")]
pub(super) async fn vllm_manage(
    UiAuthenticated(authenticated): UiAuthenticated,
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
) -> Response {
    if !authenticated {
        return crate::routers::ui::common::ui_login_redirect();
    }
    render_vllm_manage_page(can(claims.as_ref(), &config, "launch"))
}

#[get("/ui/vllm/manage/")]
pub(super) async fn vllm_manage_slash(
    UiAuthenticated(authenticated): UiAuthenticated,
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
) -> Response {
    if !authenticated {
        return crate::routers::ui::common::ui_login_redirect();
    }
    render_vllm_manage_page(can(claims.as_ref(), &config, "launch"))
}

fn render_vllm_manage_page(can_launch: bool) -> Response {
    render_page(
        http::StatusCode::OK,
        content()
            .class("vllm-manage-content")
            .child(
                div()
                    .class("top-bar")
                    .child(
                        div()
                            .attr("hx-ext", "sse")
                            .attr("sse-connect", with_base_path("/api/v1/gpu/status/sse"))
                            .child(
                                div()
                                    .class("gpu")
                                    .attr("id", "gpu-status")
                                    .attr("sse-swap", "gpu-status")
                                    .child(
                                        div()
                                            .class("gpu-name")
                                            .attr("data-i18n", "ui_gpu_unavailable")
                                            .text("GPU: n/a"),
                                    )
                                    .child(
                                        div()
                                            .class("gpu-total")
                                            .child(span().attr("data-i18n", "ui_models_gpu_total"))
                                            .child(span().text(" n/a GB")),
                                    )
                                    .child(
                                        div()
                                            .class("gpu-free")
                                            .child(span().attr("data-i18n", "ui_models_gpu_free"))
                                            .child(span().text(" n/a GB")),
                                    ),
                            ),
                    )
                    .child(div().class("flex-1"))
                    .child_opt(can_launch.then(|| {
                        a().attr("id", "launch-instance-action")
                            .class("toolbar-action")
                            .attr("href", "#launch-modal")
                            .attr("hx-get", with_base_path("/api/v1/vllm/launch-modal"))
                            .attr("hx-target", "#launch-modal")
                            .attr("hx-swap", "outerHTML")
                            .child(i().class("fa-solid fa-plus"))
                            .child(
                                span()
                                    .attr("data-i18n", "ui_vllm_launch_new")
                                    .text("Launch"),
                            )
                    })),
            )
            .child(
                div()
                    .attr("hx-ext", "sse")
                    .attr("sse-connect", with_base_path("/api/v1/vllm/sse"))
                    .child(
                        div()
                            .class("grid")
                            .attr("id", "vllm-instances-grid")
                            .attr("sse-swap", "vllm-instances")
                            .attr("hx-swap", "outerHTML"),
                    ),
            )
            .child(div().attr("id", "launch-modal").class("modal launch-modal"))
            .child(confirm_stop_instance_modal()),
        UiPageKind::VllmManagement,
    )
}

fn confirm_stop_instance_modal() -> Element {
    div()
        .attr("id", "confirm-stop-instance-modal")
        .class("estimates-modal")
}

pub(super) fn register_routes() {
    let _ = vllm_manage as fn(_, _, _) -> _;
    let _ = vllm_manage_slash as fn(_, _, _) -> _;
}
