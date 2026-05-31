use super::mod_impl::is_admin;
use super::store::get_store;
use super::types::{Model, ModelEstimate, ModelFilters};
use crate::routers::gpu::get_gpu_info;
use crate::routers::gpu::monitor::GpuInfo;
use actix_web::web::Json;
use actix_web::{HttpResponse, Responder, get, post, web};
use quench_srv::prelude::JwtConfig;
use quench_web::prelude::*;

#[post("/list")]
pub async fn handle_list(body: Json<ModelFilters>) -> impl Responder {
    let gpu = get_gpu_info().unwrap_or_default();
    let mut models = get_store().get_all_models().await;

    apply_filters(&mut models, &body, &gpu);

    HttpResponse::Ok().json(models)
}

#[get("/grid")]
pub async fn handle_grid(
    req: actix_web::HttpRequest,
    config: web::Data<JwtConfig>,
    filters: web::Query<ModelFilters>,
) -> impl Responder {
    let gpu = get_gpu_info().unwrap_or_default();
    let mut models = get_store().get_all_models().await;
    let admin = is_admin(&req, &config);

    apply_filters(&mut models, &filters, &gpu);

    let html = render_model_grid(models, &gpu, admin);

    HttpResponse::Ok().content_type("text/html").body(html)
}

pub fn apply_filters(models: &mut Vec<Model>, filters: &ModelFilters, gpu: &GpuInfo) {
    // 1. Filter by source (HF/GGUF) - default to HF if not specified or empty
    let source = filters
        .source
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or("hf");
    let source_lower = source.to_lowercase();
    models.retain(|m| m.source.to_lowercase() == source_lower);

    // 2. Filter by search term
    if let Some(search) = &filters.search {
        let search_lower = search.to_lowercase();
        if !search_lower.is_empty() {
            models.retain(|m| m.name.to_lowercase().contains(&search_lower));
        }
    }

    // 3. Filter by quantization
    if let Some(quant_str) = &filters.quant
        && quant_str != "ALL"
        && !quant_str.is_empty()
    {
        models.retain(|m| {
            // Match using debug format or aliases
            let m_quant_str = format!("{:?}", m.quant);
            m_quant_str == *quant_str
                || match m.quant {
                    super::types::Quant::Q80 => quant_str == "Q8_0",
                    super::types::Quant::Q6K => quant_str == "Q6_K",
                    super::types::Quant::Q5KM => quant_str == "Q5_K_M",
                    super::types::Quant::Q50 => quant_str == "Q5_0",
                    super::types::Quant::Q4KM => quant_str == "Q4_K_M",
                    super::types::Quant::Q40 => quant_str == "Q4_0",
                    super::types::Quant::Q3KM => quant_str == "Q3_K_M",
                    super::types::Quant::Q2K => quant_str == "Q2_K",
                    _ => false,
                }
        });
    }

    // 4. Filter by context size
    if let Some(context_val) = &filters.context {
        let context_num = match context_val {
            serde_json::Value::Number(n) => n.as_u64().unwrap_or(0) as usize,
            serde_json::Value::String(s) => s.parse::<usize>().unwrap_or(0),
            _ => 0,
        };

        if context_num != 0 {
            models.retain(|m| m.context.as_usize() >= context_num);
        }
    }

    // 5. Filter by vLLM support
    if let Some(vllm_only) = filters.vllm_only
        && vllm_only
    {
        models.retain(|m| m.vllm_supported);
    }

    // 6. Sort models
    if let Some(sort) = &filters.sort {
        match sort.as_str() {
            "name_asc" => models.sort_by(|a, b| a.name.cmp(&b.name)),
            "name_desc" => models.sort_by(|a, b| b.name.cmp(&a.name)),
            "params_asc" => models.sort_by(|a, b| {
                a.params_billion
                    .partial_cmp(&b.params_billion)
                    .unwrap_or(std::cmp::Ordering::Equal)
            }),
            "params_desc" => models.sort_by(|a, b| {
                b.params_billion
                    .partial_cmp(&a.params_billion)
                    .unwrap_or(std::cmp::Ordering::Equal)
            }),
            "vram_asc" => models.sort_by(|a, b| {
                let best_a = find_best_estimate(&a.estimates, gpu.free_gb);
                let best_b = find_best_estimate(&b.estimates, gpu.free_gb);
                let vram_a = best_a.map(|e| e.total_gb).unwrap_or(1000.0);
                let vram_b = best_b.map(|e| e.total_gb).unwrap_or(1000.0);
                vram_a
                    .partial_cmp(&vram_b)
                    .unwrap_or(std::cmp::Ordering::Equal)
            }),
            "vram_desc" => models.sort_by(|a, b| {
                let best_a = find_best_estimate(&a.estimates, gpu.free_gb);
                let best_b = find_best_estimate(&b.estimates, gpu.free_gb);
                let vram_a = best_a.map(|e| e.total_gb).unwrap_or(1000.0);
                let vram_b = best_b.map(|e| e.total_gb).unwrap_or(1000.0);
                vram_b
                    .partial_cmp(&vram_a)
                    .unwrap_or(std::cmp::Ordering::Equal)
            }),
            _ => {}
        }
    }
}

fn find_best_estimate(estimates: &[ModelEstimate], available_vram: f64) -> Option<&ModelEstimate> {
    estimates
        .iter()
        .filter(|e| e.total_gb <= available_vram)
        .max_by(|a, b| {
            let a_ctx = a.context.as_usize();
            let b_ctx = b.context.as_usize();
            if a_ctx != b_ctx {
                a_ctx.cmp(&b_ctx)
            } else {
                a.quant.rank().cmp(&b.quant.rank())
            }
        })
}

fn find_minimum_estimate(estimates: &[ModelEstimate]) -> Option<&ModelEstimate> {
    estimates.iter().min_by(|a, b| {
        a.total_gb
            .partial_cmp(&b.total_gb)
            .unwrap_or(std::cmp::Ordering::Equal)
    })
}

fn render_fit_item(key: &str, label: &str, value: Option<String>) -> Element {
    if let Some(val) = value {
        span()
            .child(element("strong").attr("data-i18n", key).text(label))
            .child(span().text(format!(": {}", val)))
    } else {
        span().attr("data-i18n", key).text(label)
    }
}

fn render_separator() -> Element {
    span().class("fit-separator").text(" | ")
}

fn render_model_grid(models: Vec<Model>, gpu: &GpuInfo, is_admin: bool) -> String {
    let mut grid = div()
        .attr("id", "models-grid")
        .attr("name", "models-grid")
        .class("models-grid grid");

    for model in models {
        let best = find_best_estimate(&model.estimates, gpu.free_gb);
        let minimum = find_minimum_estimate(&model.estimates);

        let (fit_class, fit_content) = if let Some(best) = best {
            let margin = gpu.free_gb - best.total_gb;
            let cls = if margin <= 2.0 { "fit-warn" } else { "fit-ok" };
            (
                format!("fit-line {}", cls),
                div()
                    .child(render_fit_item("ui_models_card_fits_yes", "Fits", None))
                    .child(render_separator())
                    .child(render_fit_item(
                        "ui_models_card_best",
                        "Best",
                        Some(format!("{} / {}", best.context, best.quant)),
                    ))
                    .child(render_separator())
                    .child(render_fit_item(
                        "ui_models_card_vram",
                        "VRAM",
                        Some(format!("{:.1} GB", best.total_gb)),
                    ))
                    .child(render_separator())
                    .child(render_fit_item(
                        "ui_models_card_margin",
                        "Margin",
                        Some(format!("{:.2} GB", margin)),
                    )),
            )
        } else if let Some(min) = minimum {
            (
                "fit-line fit-no".to_string(),
                div()
                    .child(render_fit_item("ui_models_card_fits_no", "Fits", None))
                    .child(render_separator())
                    .child(render_fit_item(
                        "ui_models_card_minimum",
                        "Min",
                        Some(format!("{:.1} GB", min.total_gb)),
                    ))
                    .child(render_separator()),
            )
        } else {
            ("fit-line fit-no".to_string(), div().text("No estimates"))
        };

        let mut header = div().class("card-header").child(
            div()
                .class("card-title")
                .child(if model.vllm_supported {
                    span().class("vllm-badge").text("vLLM")
                } else {
                    span().class("vllm-badge").attr("style", "display:none")
                })
                .child(span().class("card-title-text").text(&model.name)),
        );

        if is_admin {
            header = header.child(
                div()
                    .class("card-delete")
                    .attr(
                        "onclick",
                        "event.stopPropagation(); openConfirmDeleteModal(this.closest('.card'))",
                    )
                    .child(i().class("fa-solid fa-trash")),
            );
        }

        let card = div()
            .class("card")
            .attr("data-model", serde_json::to_string(&model).unwrap())
            .attr("onclick", "openEstimatesModal(this.closest('.card'))")
            .child(header)
            .child(
                div()
                    .class("card-meta")
                    .child(
                        div()
                            .child(
                                div()
                                    .class("card-meta-params")
                                    .child(span().text("Params: "))
                                    .child(span().text(format!("{:.1}B", model.params_billion))),
                            )
                            .child(
                                div()
                                    .class("card-meta-quant")
                                    .child(span().text("Quant: "))
                                    .child(span().text(model.quant.to_string())),
                            )
                            .child(
                                div()
                                    .class("card-meta-context")
                                    .child(span().text("Context: "))
                                    .child(span().text(model.context.to_string())),
                            ),
                    )
                    .child(
                        div()
                            .child(
                                div()
                                    .class("card-meta-layers")
                                    .child(span().text("Layers: "))
                                    .child(span().text(model.layers.to_string())),
                            )
                            .child(
                                div()
                                    .class("card-meta-hidden")
                                    .child(span().text("Hidden: "))
                                    .child(span().text(model.hidden_size.to_string())),
                            ),
                    ),
            )
            .child(
                div()
                    .class("card-fit")
                    .child(div().class(fit_class).child(fit_content)),
            )
            .child(div().class("card-path").text(&model.path));

        grid = grid.child(card);
    }

    grid.render()
}
