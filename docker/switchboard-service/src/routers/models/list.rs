use super::mod_impl::is_admin;
use super::store::get_store;
use super::types::{Model, ModelEstimate, ModelFilters};
use crate::routers::gpu::get_gpu_info;
use crate::routers::gpu::monitor::GpuInfo;
use actix_web::web::Json;
use actix_web::{HttpResponse, Responder, get, post, web};
use quench_auth::prelude::JwtConfig;
use quench_starter::prelude::with_base_path;
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

#[derive(Debug, serde::Deserialize)]
pub struct EstimatesModalQuery {
    path: Option<String>,
    fit: Option<String>,
    context: Option<String>,
    quant: Option<String>,
}

#[get("/estimates-modal")]
pub async fn estimates_modal(query: web::Query<EstimatesModalQuery>) -> impl Responder {
    let models = get_store().get_all_models().await;
    let gpu = get_gpu_info().unwrap_or_default();
    let model = query
        .path
        .as_deref()
        .and_then(|path| models.into_iter().find(|model| model.path == path));

    let html = match model {
        Some(model) => render_estimates_modal(&model, &gpu, &query),
        None => empty_estimates_modal(),
    };

    HttpResponse::Ok().content_type("text/html").body(html)
}

#[get("/estimates-modal/empty")]
pub async fn empty_estimates_modal_endpoint() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/html")
        .body(empty_estimates_modal())
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
                button()
                    .class("card-delete")
                    .attr("type", "button")
                    .attr("title", "Delete model")
                    .attr(
                        "hx-get",
                        format!(
                            "{}?path={}&name={}",
                            with_base_path("/api/v1/models/delete-modal"),
                            encode_query_component(&model.path),
                            encode_query_component(&model.name)
                        ),
                    )
                    .attr("hx-target", "#confirm-delete-modal")
                    .attr("hx-swap", "outerHTML")
                    .child(i().class("fa-solid fa-trash")),
            );
        }

        let card = div()
            .class("card")
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
                    .attr(
                        "hx-get",
                        format!(
                            "{}?path={}",
                            with_base_path("/api/v1/models/estimates-modal"),
                            encode_query_component(&model.path)
                        ),
                    )
                    .attr("hx-target", "#estimates-modal")
                    .attr("hx-swap", "outerHTML")
                    .child(
                        div().class(fit_class).child(fit_content).child(
                            span()
                                .class("fit-details-icon")
                                .child(i().class("fa-solid fa-chart-simple")),
                        ),
                    ),
            )
            .child(
                div()
                    .class("card-path")
                    .attr("title", &model.path)
                    .text(&model.path),
            );

        grid = grid.child(card);
    }

    grid.render()
}

#[derive(Debug, serde::Deserialize)]
pub struct DeleteModalQuery {
    path: String,
    name: Option<String>,
}

#[get("/delete-modal")]
pub async fn delete_modal(query: web::Query<DeleteModalQuery>) -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/html")
        .body(render_delete_modal(&query.path, query.name.as_deref()))
}

#[get("/delete-modal/empty")]
pub async fn empty_delete_modal_endpoint() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/html")
        .body(empty_delete_modal())
}

fn render_estimates_modal(model: &Model, gpu: &GpuInfo, query: &EstimatesModalQuery) -> String {
    let fit_filter = query.fit.as_deref().unwrap_or("all");
    let context_filter = query.context.as_deref().unwrap_or("all");
    let quant_filter = query.quant.as_deref().unwrap_or("all");

    let mut contexts = model
        .estimates
        .iter()
        .map(|estimate| estimate.context.to_string())
        .collect::<Vec<_>>();
    contexts.sort_by_key(|value| value.parse::<usize>().unwrap_or_default());
    contexts.dedup();

    let mut quants = model
        .estimates
        .iter()
        .map(|estimate| estimate.quant.to_string())
        .collect::<Vec<_>>();
    quants.sort();
    quants.dedup();

    let mut grid = div().class("estimate-grid").attr("id", "estimate-grid");
    for estimate in model.estimates.iter().filter(|estimate| {
        let fits = estimate.total_gb <= gpu.free_gb;
        (fit_filter != "fit" || fits)
            && (fit_filter != "nofit" || !fits)
            && (context_filter == "all" || estimate.context.to_string() == context_filter)
            && (quant_filter == "all" || estimate.quant.to_string() == quant_filter)
    }) {
        let margin = gpu.free_gb - estimate.total_gb;
        let fit_class = if estimate.total_gb > gpu.free_gb {
            "fit-line fit-no"
        } else if margin <= 2.0 {
            "fit-line fit-warn"
        } else {
            "fit-line fit-ok"
        };
        grid = grid.child(
            div()
                .class(fit_class)
                .child(div().child(render_fit_item(
                    "ui_models_card_context",
                    "Context",
                    Some(estimate.context.to_string()),
                )))
                .child(div().child(render_fit_item(
                    "ui_models_card_quant",
                    "Quant",
                    Some(estimate.quant.to_string()),
                )))
                .child(div().child(render_fit_item(
                    "ui_models_card_vram",
                    "VRAM",
                    Some(format!("{:.1} GB", estimate.total_gb)),
                )))
                .child(div().child(render_fit_item(
                    "ui_models_card_margin",
                    "Margin",
                    Some(format!("{:.2} GB", margin)),
                ))),
        );
    }

    div()
        .attr("id", "estimates-modal")
        .class("estimates-modal open")
        .child(modal_close_backdrop(
            &with_base_path("/api/v1/models/estimates-modal/empty"),
            "#estimates-modal",
        ))
        .child(
            div()
                .class("estimates-modal-content")
                .child(
                    div()
                        .class("estimates-modal-header")
                        .child(
                            div()
                                .class("estimates-modal-title")
                                .child(
                                    span()
                                        .attr("data-i18n", "ui_models_modal_estimates_title")
                                        .text("Estimations"),
                                )
                                .child(span().text(format!(" - {}", model.name))),
                        )
                        .child(modal_close_button(
                            &with_base_path("/api/v1/models/estimates-modal/empty"),
                            "#estimates-modal",
                        )),
                )
                .child(
                    div()
                        .class("estimates-modal-body")
                        .child(
                            form()
                                .class("estimate-filters")
                                .attr("hx-get", with_base_path("/api/v1/models/estimates-modal"))
                                .attr("hx-target", "#estimates-modal")
                                .attr("hx-swap", "outerHTML")
                                .attr("hx-trigger", "change")
                                .child(
                                    input()
                                        .attr("type", "hidden")
                                        .attr("name", "path")
                                        .attr("value", &model.path),
                                )
                                .child(select_with_options(
                                    "fit",
                                    fit_filter,
                                    &[
                                        (
                                            "all",
                                            "All",
                                            Some("ui_models_modal_estimates_filter_all"),
                                        ),
                                        (
                                            "fit",
                                            "Fits",
                                            Some("ui_models_modal_estimates_filter_fits"),
                                        ),
                                        (
                                            "nofit",
                                            "Does not fit",
                                            Some("ui_models_modal_estimates_filter_nofit"),
                                        ),
                                    ],
                                ))
                                .child(select_from_values(
                                    "context",
                                    context_filter,
                                    "all",
                                    "All Contexts",
                                    &contexts,
                                ))
                                .child(select_from_values(
                                    "quant",
                                    quant_filter,
                                    "all",
                                    "All Quants",
                                    &quants,
                                )),
                        )
                        .child(grid),
                ),
        )
        .render()
}

fn render_delete_modal(path: &str, name: Option<&str>) -> String {
    div()
        .attr("id", "confirm-delete-modal")
        .class("estimates-modal open")
        .child(modal_close_backdrop(
            &with_base_path("/api/v1/models/delete-modal/empty"),
            "#confirm-delete-modal",
        ))
        .child(
            div()
                .class("estimates-modal-content small")
                .child(
                    div()
                        .class("estimates-modal-header")
                        .child(
                            div()
                                .class("estimates-modal-title")
                                .attr("data-i18n", "ui_models_modal_delete_title")
                                .text("Confirm Delete"),
                        )
                        .child(modal_close_button(
                            &with_base_path("/api/v1/models/delete-modal/empty"),
                            "#confirm-delete-modal",
                        )),
                )
                .child(
                    div()
                        .class("estimates-modal-body")
                        .child(
                            p().attr("data-i18n", "ui_models_modal_delete_text")
                                .text("Are you sure you want to delete this model?"),
                        )
                        .child(
                            div()
                                .class("model-to-delete-name")
                                .text(name.unwrap_or(path)),
                        )
                        .child(
                            form()
                                .class("confirm-actions")
                                .attr("hx-post", with_base_path("/api/v1/models/delete-form"))
                                .attr("hx-target", "#confirm-delete-modal")
                                .attr("hx-swap", "outerHTML")
                                .child(
                                    input()
                                        .attr("type", "hidden")
                                        .attr("name", "path")
                                        .attr("value", path),
                                )
                                .child(
                                    button()
                                        .class("button cancel")
                                        .attr("type", "button")
                                        .attr("data-i18n", "ui_common_cancel")
                                        .attr(
                                            "hx-get",
                                            with_base_path("/api/v1/models/delete-modal/empty"),
                                        )
                                        .attr("hx-target", "#confirm-delete-modal")
                                        .attr("hx-swap", "outerHTML")
                                        .text("Cancel"),
                                )
                                .child(
                                    button()
                                        .class("button delete")
                                        .attr("type", "submit")
                                        .attr("data-i18n", "ui_models_modal_delete_confirm")
                                        .text("Delete"),
                                ),
                        ),
                ),
        )
        .render()
}

fn modal_close_backdrop(endpoint: &str, target: &str) -> Element {
    button()
        .class("estimates-modal-backdrop")
        .attr("type", "button")
        .attr("hx-get", endpoint)
        .attr("hx-target", target)
        .attr("hx-swap", "outerHTML")
}

fn modal_close_button(endpoint: &str, target: &str) -> Element {
    button()
        .class("estimates-modal-close")
        .attr("type", "button")
        .attr("hx-get", endpoint)
        .attr("hx-target", target)
        .attr("hx-swap", "outerHTML")
        .child(i().class("fa-solid fa-xmark"))
}

fn empty_estimates_modal() -> String {
    div()
        .attr("id", "estimates-modal")
        .class("estimates-modal")
        .render()
}

fn empty_delete_modal() -> String {
    div()
        .attr("id", "confirm-delete-modal")
        .class("estimates-modal")
        .render()
}

fn select_with_options(
    name: &str,
    selected: &str,
    options: &[(&str, &str, Option<&str>)],
) -> Element {
    let mut select = select().attr("name", name);
    for (value, label_text, i18n) in options {
        let mut option = option().attr("value", value).text(label_text);
        if selected == *value {
            option = option.attr("selected", "selected");
        }
        if let Some(key) = i18n {
            option = option.attr("data-i18n", key);
        }
        select = select.child(option);
    }
    select
}

fn select_from_values(
    name: &str,
    selected: &str,
    all_value: &str,
    all_label: &str,
    values: &[String],
) -> Element {
    let mut select = select().attr("name", name);
    let mut all = option().attr("value", all_value).text(all_label);
    if selected == all_value {
        all = all.attr("selected", "selected");
    }
    select = select.child(all);
    for value in values {
        let mut opt = option().attr("value", value).text(value);
        if selected == value {
            opt = opt.attr("selected", "selected");
        }
        select = select.child(opt);
    }
    select
}

fn encode_query_component(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}
