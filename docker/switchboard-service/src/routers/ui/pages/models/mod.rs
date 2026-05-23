use crate::routers::ui::common::{self, UiPageKind, render_page};
use actix_web::{HttpResponse, Responder, get, web};
use quench_srv::prelude::jwt::JwtConfig;
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
                            .class("gpu")
                            .attr("id", "gpu-status")
                            .child(div().class("gpu-name").text("GPU: n/a"))
                            .child(div().class("gpu-total").text("Total: n/a GB"))
                            .child(div().class("gpu-free").text("Free: n/a GB")),
                    )
                    .child(div().class("flex-1"))
                    .child(
                        div()
                            .class("model-tabs")
                            .child(
                                div()
                                    .attr("id", "model-tab-hf")
                                    .class("tab active")
                                    .text("HF")
                                    .on_click("toggleModelSource(event)"),
                            )
                            .child(
                                div()
                                    .attr("id", "model-tab-gguf")
                                    .class("tab")
                                    .text("GGUF")
                                    .on_click("toggleModelSource(event)"),
                            ),
                    )
                    .child(
                        div()
                            .class("model-filters")
                            .child(input().attr("id", "search").attr("placeholder", "search"))
                            .child(
                                select()
                                    .attr("id", "quant")
                                    .child(option().attr("value", "ALL").text("all quants"))
                                    // HF
                                    .child(
                                        option()
                                            .class("quant-hf")
                                            .attr("value", "FP16")
                                            .text("fp16"),
                                    )
                                    .child(
                                        option()
                                            .class("quant-hf")
                                            .attr("value", "BF16")
                                            .text("bf16"),
                                    )
                                    .child(
                                        option().class("quant-hf").attr("value", "FP8").text("fp8"),
                                    )
                                    .child(
                                        option()
                                            .class("quant-hf")
                                            .attr("value", "INT8")
                                            .text("int8"),
                                    )
                                    .child(
                                        option().class("quant-hf").attr("value", "AWQ").text("awq"),
                                    )
                                    .child(
                                        option()
                                            .class("quant-hf")
                                            .attr("value", "GPTQ")
                                            .text("gptq"),
                                    )
                                    // GGUF
                                    .child(
                                        option()
                                            .class("quant-gguf hidden")
                                            .attr("value", "Q8_0")
                                            .text("q8_0"),
                                    )
                                    .child(
                                        option()
                                            .class("quant-gguf hidden")
                                            .attr("value", "Q6_K")
                                            .text("q6_k"),
                                    )
                                    .child(
                                        option()
                                            .class("quant-gguf hidden")
                                            .attr("value", "Q5_K_M")
                                            .text("q5_k_m"),
                                    )
                                    .child(
                                        option()
                                            .class("quant-gguf hidden")
                                            .attr("value", "Q5_0")
                                            .text("q5_0"),
                                    )
                                    .child(
                                        option()
                                            .class("quant-gguf hidden")
                                            .attr("value", "Q4_K_M")
                                            .text("q4_k_m"),
                                    )
                                    .child(
                                        option()
                                            .class("quant-gguf hidden")
                                            .attr("value", "Q4_0")
                                            .text("q4_0"),
                                    )
                                    .child(
                                        option()
                                            .class("quant-gguf hidden")
                                            .attr("value", "Q3_K_M")
                                            .text("q3_k_m"),
                                    )
                                    .child(
                                        option()
                                            .class("quant-gguf hidden")
                                            .attr("value", "Q2_K")
                                            .text("q2_k"),
                                    )
                                    .on_change("onChangeQuant()"),
                            )
                            .child(
                                select()
                                    .attr("id", "context")
                                    .child(option().attr("value", "0").text("all contexts"))
                                    .child(option().attr("value", "512").text("512"))
                                    .child(option().attr("value", "1024").text("1024"))
                                    .child(option().attr("value", "2048").text("2048"))
                                    .child(option().attr("value", "4096").text("4096"))
                                    .child(option().attr("value", "8192").text("8192"))
                                    .child(option().attr("value", "16384").text("16384"))
                                    .child(option().attr("value", "32768").text("32768"))
                                    .child(option().attr("value", "65536").text("65536"))
                                    .child(option().attr("value", "131072").text("131072"))
                                    .on_change("onChangeContext()"),
                            ),
                    ),
            )
            .child(div().class("grid").attr("id", "models-grid")),
        UiPageKind::ModelsDashboard,
    )
}
