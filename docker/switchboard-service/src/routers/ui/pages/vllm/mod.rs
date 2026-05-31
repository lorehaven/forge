use crate::routers::ui::common;
use crate::routers::ui::common::{UiPageKind, render_page};
use actix_web::{HttpResponse, Responder, get, web};
use quench_srv::prelude::{JwtConfig, with_base_path};
use quench_web::prelude::*;

#[get("/vllm/manage")]
pub(super) async fn vllm_manage(
    req: actix_web::HttpRequest,
    config: web::Data<JwtConfig>,
) -> impl Responder {
    if !common::is_ui_authenticated(&req, &config) {
        return common::ui_login_redirect();
    }
    render_vllm_manage_page(crate::routers::models::mod_impl::is_admin(&req, &config))
}

#[get("/vllm/manage/")]
pub(super) async fn vllm_manage_slash(
    req: actix_web::HttpRequest,
    config: web::Data<JwtConfig>,
) -> impl Responder {
    if !common::is_ui_authenticated(&req, &config) {
        return common::ui_login_redirect();
    }
    render_vllm_manage_page(crate::routers::models::mod_impl::is_admin(&req, &config))
}

fn render_vllm_manage_page(is_admin: bool) -> HttpResponse {
    render_page(
        HttpResponse::Ok(),
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
                                    .child(div().class("gpu-name").text("GPU: n/a"))
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
                    .child_opt(is_admin.then(|| {
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
                    .attr("sse-connect", with_base_path("/api/v1/vllm/instances/sse"))
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
