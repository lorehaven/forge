use crate::routers::ui::common::{self, UiPageKind, render_page};
use actix_web::{HttpResponse, Responder, get, web};
use quench_auth::prelude::JwtConfig;
use quench_starter::prelude::with_base_path;
use quench_web::prelude::*;

#[get("/models/dashboard")]
pub(super) async fn models_dashboard(
    req: actix_web::HttpRequest,
    config: web::Data<JwtConfig>,
) -> impl Responder {
    if !common::is_ui_authenticated(&req, &config) {
        return common::ui_login_redirect();
    }
    render_models_dashboard_page()
}

#[get("/models/dashboard/")]
pub(super) async fn models_dashboard_slash(
    req: actix_web::HttpRequest,
    config: web::Data<JwtConfig>,
) -> impl Responder {
    if !common::is_ui_authenticated(&req, &config) {
        return common::ui_login_redirect();
    }
    render_models_dashboard_page()
}

fn render_models_dashboard_page() -> HttpResponse {
    render_page(
        HttpResponse::Ok(),
        content()
            .class("models-dashboard-content")
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
                    .child(
                        form()
                            .attr("id", "model-filters")
                            .attr("hx-get", with_base_path("/api/v1/models/grid"))
                            .attr("hx-target", "#models-grid")
                            .attr("hx-swap", "outerHTML")
                            .attr(
                                "hx-trigger",
                                "change, keyup changed delay:300ms from:#search, models-refresh from:body",
                            )
                            .child(
                                select()
                                    .attr("name", "source")
                                    .attr("id", "source")
                                    .child(
                                        option()
                                            .attr("value", "hf")
                                            .attr("selected", "selected")
                                            .attr("data-i18n", "ui_models_tab_hf"),
                                    )
                                    .child(
                                        option()
                                            .attr("value", "gguf")
                                            .attr("data-i18n", "ui_models_tab_gguf"),
                                    ),
                            )
                            .child(
                                select()
                                    .attr("id", "sort")
                                    .attr("name", "sort")
                                    .child(
                                        option()
                                            .attr("value", "name_asc")
                                            .attr("data-i18n", "ui_models_sort_name_asc"),
                                    )
                                    .child(
                                        option()
                                            .attr("value", "name_desc")
                                            .attr("data-i18n", "ui_models_sort_name_desc"),
                                    )
                                    .child(
                                        option()
                                            .attr("value", "params_asc")
                                            .attr("data-i18n", "ui_models_sort_params_asc"),
                                    )
                                    .child(
                                        option()
                                            .attr("value", "params_desc")
                                            .attr("data-i18n", "ui_models_sort_params_desc"),
                                    )
                                    .child(
                                        option()
                                            .attr("value", "vram_asc")
                                            .attr("data-i18n", "ui_models_sort_vram_asc"),
                                    )
                                    .child(
                                        option()
                                            .attr("value", "vram_desc")
                                            .attr("data-i18n", "ui_models_sort_vram_desc"),
                                    ),
                            )
                            .child(
                                div()
                                    .class("model-filters")
                                    .child(
                                        input()
                                            .attr("id", "search")
                                            .attr("name", "search")
                                            .attr("placeholder", "search")
                                            .attr(
                                                "data-i18n-placeholder",
                                                "ui_models_search_placeholder",
                                            ),
                                    )
                                    .child(
                                        select()
                                            .attr("id", "quant")
                                            .attr("name", "quant")
                                            .child(
                                                option().attr("value", "ALL").attr(
                                                    "data-i18n",
                                                    "ui_models_filter_all_quants",
                                                ),
                                            )
                                            // HF
                                            .child(
                                                option()
                                                    .class("quant-hf")
                                                    .attr("value", "FP16")
                                                    .attr("data-i18n", "ui_models_quant_fp16"),
                                            )
                                            .child(
                                                option()
                                                    .class("quant-hf")
                                                    .attr("value", "BF16")
                                                    .attr("data-i18n", "ui_models_quant_bf16"),
                                            )
                                            .child(
                                                option()
                                                    .class("quant-hf")
                                                    .attr("value", "FP8")
                                                    .attr("data-i18n", "ui_models_quant_fp8"),
                                            )
                                            .child(
                                                option()
                                                    .class("quant-hf")
                                                    .attr("value", "INT8")
                                                    .attr("data-i18n", "ui_models_quant_int8"),
                                            )
                                            .child(
                                                option()
                                                    .class("quant-hf")
                                                    .attr("value", "AWQ")
                                                    .attr("data-i18n", "ui_models_quant_awq"),
                                            )
                                            .child(
                                                option()
                                                    .class("quant-hf")
                                                    .attr("value", "GPTQ")
                                                    .attr("data-i18n", "ui_models_quant_gptq"),
                                            )
                                            // GGUF
                                            .child(
                                                option()
                                                    .class("quant-gguf hidden")
                                                    .attr("value", "Q8_0")
                                                    .attr("data-i18n", "ui_models_quant_q8_0"),
                                            )
                                            .child(
                                                option()
                                                    .class("quant-gguf hidden")
                                                    .attr("value", "Q6_K")
                                                    .attr("data-i18n", "ui_models_quant_q6_k"),
                                            )
                                            .child(
                                                option()
                                                    .class("quant-gguf hidden")
                                                    .attr("value", "Q5_K_M")
                                                    .attr("data-i18n", "ui_models_quant_q5_k_m"),
                                            )
                                            .child(
                                                option()
                                                    .class("quant-gguf hidden")
                                                    .attr("value", "Q5_0")
                                                    .attr("data-i18n", "ui_models_quant_q5_0"),
                                            )
                                            .child(
                                                option()
                                                    .class("quant-gguf hidden")
                                                    .attr("value", "Q4_K_M")
                                                    .attr("data-i18n", "ui_models_quant_q4_k_m"),
                                            )
                                            .child(
                                                option()
                                                    .class("quant-gguf hidden")
                                                    .attr("value", "Q4_0")
                                                    .attr("data-i18n", "ui_models_quant_q4_0"),
                                            )
                                            .child(
                                                option()
                                                    .class("quant-gguf hidden")
                                                    .attr("value", "Q3_K_M")
                                                    .attr("data-i18n", "ui_models_quant_q3_k_m"),
                                            )
                                            .child(
                                                option()
                                                    .class("quant-gguf hidden")
                                                    .attr("value", "Q2_K")
                                                    .attr("data-i18n", "ui_models_quant_q2_k"),
                                            ),
                                    )
                                    .child(
                                        select()
                                            .attr("id", "context")
                                            .attr("name", "context")
                                            .child(
                                                option().attr("value", "0").attr(
                                                    "data-i18n",
                                                    "ui_models_filter_all_contexts",
                                                ),
                                            )
                                            .child(option().attr("value", "512").text("512"))
                                            .child(option().attr("value", "1024").text("1024"))
                                            .child(option().attr("value", "2048").text("2048"))
                                            .child(option().attr("value", "4096").text("4096"))
                                            .child(option().attr("value", "8192").text("8192"))
                                            .child(option().attr("value", "16384").text("16384"))
                                            .child(option().attr("value", "32768").text("32768"))
                                            .child(option().attr("value", "65536").text("65536"))
                                            .child(option().attr("value", "131072").text("131072")),
                                    )
                                    .child(
                                        label()
                                            .class("vllm-filter")
                                            .child(
                                                input()
                                                    .attr("type", "checkbox")
                                                    .attr("id", "vllm-only")
                                                    .attr("name", "vllm_only")
                                                    .attr("value", "true"),
                                            )
                                            .child(
                                                span().attr(
                                                    "data-i18n",
                                                    "ui_models_filter_vllm_only",
                                                ),
                                            ),
                                    ),
                            ),
                    ),
            )
            .child(
                div()
                    .class("grid")
                    .attr("id", "models-grid")
                    .attr("name", "models-grid")
                    .attr("hx-get", with_base_path("/api/v1/models/grid"))
                    .attr("hx-trigger", "load")
                    .attr("hx-swap", "outerHTML")
                    .attr("hx-include", "#model-filters"),
            )
            .child(estimates_modal())
            .child(confirm_delete_modal()),
        UiPageKind::ModelsDashboard,
    )
}

fn estimates_modal() -> Element {
    div().attr("id", "estimates-modal").class("estimates-modal")
}

fn confirm_delete_modal() -> Element {
    div()
        .attr("id", "confirm-delete-modal")
        .class("estimates-modal")
}
