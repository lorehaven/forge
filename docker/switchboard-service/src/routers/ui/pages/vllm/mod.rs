use crate::routers::ui::common;
use crate::routers::ui::common::{UiPageKind, render_page};
use actix_web::{HttpResponse, Responder, get, web};
use quench_srv::prelude::jwt::JwtConfig;
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

fn render_vllm_manage_page() -> HttpResponse {
    render_page(
        HttpResponse::Ok(),
        content()
            .class("vllm-manage-content")
            .child(
                div()
                    .class("top-bar")
                    .child(
                        div()
                            .class("gpu")
                            .attr("id", "gpu-status")
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
            .child(div().class("grid").attr("id", "vllm-instances-grid"))
            .child(vllm_empty_state_template())
            .child(vllm_instance_card_template())
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

fn vllm_empty_state_template() -> Element {
    div()
        .attr("id", "vllm-empty-state-template")
        .attr("style", "display: none;")
        .attr("aria-hidden", "true")
        .child(div().class("empty").text("No running instances"))
}

fn vllm_instance_card_template() -> Element {
    div()
        .attr("id", "vllm-instance-card-template")
        .attr("style", "display: none;")
        .attr("aria-hidden", "true")
        .child(
            div()
                .class("card")
                .child(
                    div()
                        .class("card-header")
                        .child(div().class("card-title"))
                        .child(
                            button()
                                .class("card-delete")
                                .attr("type", "button")
                                .attr("title", "Stop instance")
                                .child(i().class("fa-solid fa-power-off")),
                        ),
                )
                .child(
                    div()
                        .class("card-meta")
                        .child(
                            div()
                                .class("meta-item")
                                .child(span().text("ID"))
                                .text(" ")
                                .child(span().class("instance-id")),
                        )
                        .child(
                            div()
                                .class("meta-item")
                                .child(span().text("Endpoint"))
                                .text(" ")
                                .child(span().class("instance-endpoint")),
                        )
                        .child(
                            div()
                                .class("meta-item")
                                .child(span().text("Status"))
                                .text(" ")
                                .child(span().class("instance-status")),
                        )
                        .child(
                            div()
                                .class("meta-item")
                                .child(span().text("Started"))
                                .text(" ")
                                .child(span().class("instance-started")),
                        ),
                )
                .child(
                    div()
                        .class("card-fit")
                        .child(div().class("fit-line fit-ok")),
                )
                .child(
                    div()
                        .class("instance-diagnostics")
                        .attr("style", "display: none;"),
                ),
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
