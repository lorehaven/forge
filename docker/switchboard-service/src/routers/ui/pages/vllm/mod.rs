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
    render_vllm_manage_page()
}

#[get("/vllm/manage/")]
pub(super) async fn vllm_manage_slash(
    req: actix_web::HttpRequest,
    config: web::Data<JwtConfig>,
) -> impl Responder {
    if !common::is_ui_authenticated(&req, &config) {
        return common::ui_login_redirect();
    }
    render_vllm_manage_page()
}

fn get_vllm_namespace() -> String {
    use crate::routers::vllm::engine::VllmManagementMode;
    match VllmManagementMode::from_env() {
        VllmManagementMode::Kubernetes => std::env::var("VLLM_K8S_NAMESPACE")
            .ok()
            .or_else(|| {
                std::fs::read_to_string("/var/run/secrets/kubernetes.io/serviceaccount/namespace")
                    .ok()
            })
            .unwrap_or_else(|| "default".to_string())
            .trim()
            .to_string(),
        VllmManagementMode::Native => "native".to_string(),
    }
}

fn render_vllm_manage_page() -> HttpResponse {
    let current_namespace = get_vllm_namespace();
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
                    .child(
                        div()
                            .attr("id", "launch-instance-action")
                            .class("toolbar-action")
                            .child(i().class("fa-solid fa-plus"))
                            .child(
                                span()
                                    .attr("data-i18n", "ui_vllm_launch_new")
                                    .text("Launch"),
                            )
                            .on_click("openLaunchModal()"),
                    ),
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
                    )
            )
            .child(
                div().attr("id", "launch-modal").class("modal launch-modal").child(
                    div()
                        .class("modal-content launch-modal-content")
                        .child(
                            div()
                                .class("modal-header")
                                .child(
                                    div()
                                        .class("launch-modal-heading")
                                        .child(
                                            h3().attr(
                                                "data-i18n",
                                                "ui_vllm_launch_modal_title",
                                            ),
                                        )
                                        .child(
                                            p()
                                                .class("launch-modal-subtitle")
                                                .text(
                                                    "Configure an endpoint, memory budget, and optional runtime quantization.",
                                                ),
                                        ),
                                )
                                .child(
                                    button()
                                        .class("modal-close")
                                        .text("×")
                                        .on_click("closeLaunchModal()"),
                                ),
                        )
                        .child(
                            div()
                                .class("modal-body")
                                .child(
                                    div()
                                        .class("form-group")
                                        .child(label().attr("data-i18n", "ui_vllm_form_model"))
                                        .child(
                                            select()
                                                .attr("id", "launch-model")
                                                .on_change("onLaunchModelChange()"),
                                        ),
                                )
                                .child(
                                    div()
                                        .class("form-row launch-form-row")
                                        .child(
                                            div()
                                                .class("form-group")
                                                .child(
                                                    label().attr("data-i18n", "ui_vllm_form_host"),
                                                )
                                                .child(
                                                    input()
                                                        .attr("type", "text")
                                                        .attr("id", "launch-host")
                                                        .attr("value", "0.0.0.0"),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .class("form-group")
                                                .child(
                                                    label().attr("data-i18n", "ui_vllm_form_port"),
                                                )
                                                .child(
                                                    input()
                                                        .attr("type", "number")
                                                        .attr("id", "launch-port")
                                                        .attr("value", "8000"),
                                                ),
                                        ),
                                )
                                .child({
                                    let mut row = div().class("form-row launch-form-row");
                                    if current_namespace != "native" {
                                        row = row.child(
                                            div()
                                                .class("form-group")
                                                .child(
                                                    label().attr("data-i18n", "ui_vllm_form_namespace"),
                                                )
                                                .child(
                                                    input()
                                                        .attr("type", "text")
                                                        .attr("id", "launch-namespace")
                                                        .attr("placeholder", &current_namespace),
                                                ),
                                        );
                                    } else {
                                        row = row.child(
                                            input()
                                                .attr("type", "hidden")
                                                .attr("id", "launch-namespace")
                                                .attr("value", "native")
                                        );
                                    }
                                    row
                                })
                                .child(
                                    div()
                                        .class("form-row launch-form-row")
                                        .child(
                                            div()
                                                .class("form-group")
                                                .child(
                                                    label().attr("data-i18n", "ui_vllm_form_quant"),
                                                )
                                                .child(
                                                    select()
                                                        .attr("id", "launch-quant")
                                                        .child(
                                                            option()
                                                                .attr("value", "")
                                                                .text("auto"),
                                                        )
                                                        .child(
                                                            option()
                                                                .attr("value", "awq")
                                                                .text("awq"),
                                                        )
                                                        .child(
                                                            option()
                                                                .attr("value", "gptq")
                                                                .text("gptq"),
                                                        )
                                                        .child(
                                                            option()
                                                                .attr("value", "awq_marlin")
                                                                .text("awq_marlin"),
                                                        )
                                                        .child(
                                                            option()
                                                                .attr("value", "gptq_marlin")
                                                                .text("gptq_marlin"),
                                                        )
                                                        .child(
                                                            option()
                                                                .attr("value", "fp8")
                                                                .text("fp8"),
                                                        )
                                                        .child(
                                                            option()
                                                                .attr("value", "bitsandbytes")
                                                                .text("bitsandbytes"),
                                                        ),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .class("form-group")
                                                .child(
                                                    label()
                                                        .attr("data-i18n", "ui_vllm_form_max_len"),
                                                )
                                                .child(
                                                    input()
                                                        .attr("type", "number")
                                                        .attr("id", "launch-max-len")
                                                        .attr("placeholder", "auto"),
                                                ),
                                        ),
                                )
                                .child(
                                    div()
                                        .class("form-row launch-form-row")
                                        .child(
                                            div()
                                                .class("form-group")
                                                .child(
                                                    label()
                                                        .attr("data-i18n", "ui_vllm_form_gpu_util"),
                                                )
                                                .child(
                                                    input()
                                                        .attr("type", "number")
                                                        .attr("id", "launch-gpu-util")
                                                        .attr("step", "0.05")
                                                        .attr("min", "0.1")
                                                        .attr("max", "1.0")
                                                        .attr("value", "0.90"),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .class("form-group form-group-checkbox")
                                                .child(
                                                    label()
                                                        .class("checkbox-control")
                                                        .child(
                                                            input()
                                                                .attr("type", "checkbox")
                                                                .attr("id", "launch-prefix-caching"),
                                                        )
                                                        .child(
                                                            span()
                                                                .class("checkbox-copy")
                                                                .attr(
                                                                    "data-i18n",
                                                                    "ui_vllm_form_prefix_caching",
                                                                ),
                                                        ),
                                                ),
                                        ),
                                )
                                .child(div().attr("id", "launch-fit-note").class("fit-note")),
                        )
                        .child(
                            div()
                                .class("modal-footer")
                                .child(
                                    button()
                                        .class("button")
                                        .attr("data-i18n", "ui_common_cancel")
                                        .on_click("closeLaunchModal()"),
                                )
                                .child(
                                    button()
                                        .class("button primary")
                                        .attr("id", "confirm-launch-btn")
                                        .attr("data-i18n", "ui_vllm_launch_confirm")
                                        .on_click("launchVllmInstance()"),
                                ),
                        ),
                ),
            )
            .child(confirm_stop_instance_modal()),
        UiPageKind::VllmManagement,
    )
}

fn confirm_stop_instance_modal() -> Element {
    div()
        .attr("id", "confirm-stop-instance-modal")
        .class("estimates-modal")
        .child(
            div()
                .class("estimates-modal-backdrop")
                .attr("onclick", "closeStopInstanceModal()"),
        )
        .child(
            div()
                .class("estimates-modal-content small")
                .child(
                    div()
                        .class("estimates-modal-header")
                        .child(
                            div()
                                .class("estimates-modal-title")
                                .text("Stop vLLM Instance"),
                        )
                        .child(
                            button()
                                .class("estimates-modal-close")
                                .attr("type", "button")
                                .on_click("closeStopInstanceModal()")
                                .child(i().class("fa-solid fa-xmark")),
                        ),
                )
                .child(
                    div()
                        .class("estimates-modal-body")
                        .child(p().text("Are you sure you want to stop this instance?"))
                        .child(
                            div()
                                .class("model-to-delete-name")
                                .attr("id", "instance-to-stop-name"),
                        )
                        .child(
                            div()
                                .class("confirm-actions")
                                .child(
                                    button()
                                        .class("button cancel")
                                        .attr("type", "button")
                                        .text("Cancel")
                                        .on_click("closeStopInstanceModal()"),
                                )
                                .child(
                                    button()
                                        .class("button delete")
                                        .attr("type", "button")
                                        .text("Stop Instance")
                                        .on_click("confirmStopInstance()"),
                                ),
                        ),
                ),
        )
}
