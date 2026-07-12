use crate::routers::gpu::get_gpu_info;
use crate::routers::models::store::get_store;
use actix_web::{HttpResponse, Responder, get, web};
use quench_starter::prelude::with_base_path;
use quench_web::prelude::*;

#[derive(serde::Deserialize)]
pub struct LaunchModalQuery {
    pub model: Option<String>,
    pub host: Option<String>,
    pub port: Option<String>,
    pub namespace: Option<String>,
    pub quantization: Option<String>,
    pub max_model_len: Option<String>,
    pub gpu_memory_utilization: Option<String>,
    pub prefix_caching: Option<bool>,
    pub task: Option<String>,
    pub enable_tool_calling: Option<bool>,
    pub recalculate_gpu_util: Option<bool>,
}

#[derive(serde::Deserialize)]
pub struct StopModalQuery {
    pub id: String,
    pub model: Option<String>,
}

#[get("/launch-modal")]
pub async fn handle_launch_modal(query: web::Query<LaunchModalQuery>) -> impl Responder {
    let models = get_store().get_all_models().await;
    let gpu = get_gpu_info().unwrap_or_default();
    let html = format!(
        "<!-- launch-instance-modal -->{}",
        render_launch_modal(models, &query, &gpu)
    );
    HttpResponse::Ok().content_type("text/html").body(html)
}

#[get("/launch-modal/empty")]
pub async fn empty_launch_modal() -> impl Responder {
    HttpResponse::Ok().content_type("text/html").body(format!(
        "<!-- launch-instance-modal -->{}",
        div()
            .attr("id", "launch-modal")
            .attr("data-testid", "launch-instance-modal")
            .class("modal launch-modal launch-instance-modal")
            .render()
    ))
}

#[get("/stop-modal")]
pub async fn handle_stop_modal(query: web::Query<StopModalQuery>) -> impl Responder {
    let model = query.model.as_deref().unwrap_or("Unknown Model");
    let html = format!(
        "<!-- stop-instance-modal -->{}",
        render_stop_modal(&query.id, model)
    );
    HttpResponse::Ok().content_type("text/html").body(html)
}

#[get("/stop-modal/empty")]
pub async fn empty_stop_modal() -> impl Responder {
    HttpResponse::Ok().content_type("text/html").body(format!(
        "<!-- stop-instance-modal -->{}",
        div()
            .attr("id", "confirm-stop-instance-modal")
            .attr("data-testid", "stop-instance-modal")
            .class("estimates-modal stop-instance-modal")
            .render()
    ))
}

fn render_launch_modal(
    models: Vec<crate::routers::models::Model>,
    query: &LaunchModalQuery,
    gpu: &crate::routers::gpu::monitor::GpuInfo,
) -> String {
    let selected_model = query
        .model
        .as_deref()
        .and_then(|name| models.iter().find(|model| model.name == name));
    let quantization = query.quantization.as_deref().unwrap_or("");
    let max_model_len = parse_optional_u32(query.max_model_len.as_deref());
    let gpu_util = launch_gpu_util(selected_model, quantization, max_model_len, query, gpu);
    let prefix_caching = query.prefix_caching.unwrap_or(false);

    let (fit_note_class, fit_note_text, fit_note_i18n, launch_disabled) =
        launch_fit_note(selected_model, quantization, max_model_len, gpu_util, gpu);

    div()
        .attr("id", "launch-modal")
        .attr("data-testid", "launch-instance-modal")
        .class("modal launch-modal launch-instance-modal open")
        .child(
            div()
                .class("modal-content launch-modal-content")
                .child(
                    div()
                        .class("modal-header")
                        .child(
                            div()
                                .class("launch-modal-heading")
                                .child(
                                    h3().attr("data-i18n", "ui_vllm_launch_modal_title")
                                        .text("Launch vLLM Instance"),
                                )
                                .child(
                                    p().class("launch-modal-subtitle").text(
                                        "Configure an endpoint, memory budget, and optional runtime quantization.",
                                    ),
                                ),
                        )
                        .child(close_launch_button()),
                )
                .child(
                    div().class("modal-body").child(
                        form()
                            .attr("id", "launch-form")
                            .attr("hx-get", with_base_path("/api/v1/vllm/launch-modal"))
                            .attr("hx-target", "#launch-modal")
                            .attr("hx-swap", "outerHTML")
                            .attr("hx-trigger", "change")
                            .child(model_select(&models, query.model.as_deref()))
                            .child(endpoint_fields(query))
                            .child(namespace_field(query))
                            .child(runtime_fields(query))
                            .child(memory_fields(gpu_util, prefix_caching))
                            .child(task_fields(query))
                            .child(
                                div().attr("id", "launch-fit-note").class("fit-note").child(
                                    div()
                                        .class(fit_note_class)
                                        .child(i().class(fit_note_icon_class(fit_note_class)))
                                        .child(
                                            span()
                                                .attr("data-i18n", fit_note_i18n)
                                                .text(fit_note_text),
                                        ),
                                ),
                            ),
                    ),
                )
                .child(
                    div()
                        .class("modal-footer")
                        .child(
                            button()
                                .class("button")
                                .attr("type", "button")
                                .attr("data-i18n", "ui_common_cancel")
                                .attr("hx-get", with_base_path("/api/v1/vllm/launch-modal/empty"))
                                .attr("hx-target", "#launch-modal")
                                .attr("hx-swap", "outerHTML")
                                .text("Cancel"),
                        )
                        .child({
                            let mut btn = button()
                                .class("button primary")
                                .attr("id", "confirm-launch-btn")
                                .attr("type", "button")
                                .attr("data-i18n", "ui_vllm_launch_confirm")
                                .attr("hx-post", with_base_path("/api/v1/vllm/instances/form"))
                                .attr("hx-include", "#launch-form")
                                .attr("hx-target", "#launch-modal")
                                .attr("hx-swap", "outerHTML")
                                .text("Launch");
                            if launch_disabled {
                                btn = btn.attr("disabled", "disabled");
                            }
                            btn
                        }),
                ),
        )
        .render()
}

fn model_select(models: &[crate::routers::models::Model], selected: Option<&str>) -> Element {
    let mut select = select()
        .attr("id", "launch-model")
        .attr("name", "model")
        .attr("hx-get", with_base_path("/api/v1/vllm/launch-modal"))
        .attr("hx-target", "#launch-modal")
        .attr("hx-swap", "outerHTML")
        .attr("hx-include", "#launch-form")
        .attr("hx-vals", r#"{"recalculate_gpu_util": true}"#)
        .child(option().attr("value", "").text("-- select model --"));
    for model in models.iter().filter(|model| model.vllm_supported) {
        let mut opt = option().attr("value", &model.name).text(&model.name);
        if selected == Some(model.name.as_str()) {
            opt = opt.attr("selected", "selected");
        }
        select = select.child(opt);
    }

    div()
        .class("form-group")
        .child(
            label()
                .attr("data-i18n", "ui_vllm_form_model")
                .text("Model"),
        )
        .child(select)
}

fn endpoint_fields(query: &LaunchModalQuery) -> Element {
    div()
        .class("form-row launch-form-row compact")
        .child(
            div()
                .class("form-group launch-host-field")
                .child(label().attr("data-i18n", "ui_vllm_form_host").text("Host"))
                .child(
                    input()
                        .attr("type", "text")
                        .attr("id", "launch-host")
                        .attr("name", "host")
                        .attr("value", query.host.as_deref().unwrap_or("0.0.0.0")),
                ),
        )
        .child(
            div()
                .class("form-group launch-port-field")
                .child(label().attr("data-i18n", "ui_vllm_form_port").text("Port"))
                .child(
                    input()
                        .attr("type", "number")
                        .attr("id", "launch-port")
                        .attr("name", "port")
                        .attr("value", query.port.as_deref().unwrap_or("8000")),
                ),
        )
}

fn namespace_field(query: &LaunchModalQuery) -> Element {
    let current_namespace = get_vllm_namespace();
    if current_namespace == "native" {
        return input()
            .attr("type", "hidden")
            .attr("id", "launch-namespace")
            .attr("name", "namespace")
            .attr("value", "native");
    }

    div().class("form-row launch-form-row compact").child(
        div()
            .class("form-group launch-namespace-field")
            .child(
                label()
                    .attr("data-i18n", "ui_vllm_form_namespace")
                    .text("Namespace"),
            )
            .child(
                input()
                    .attr("type", "text")
                    .attr("id", "launch-namespace")
                    .attr("name", "namespace")
                    .attr(
                        "value",
                        query.namespace.as_deref().unwrap_or(&current_namespace),
                    ),
            ),
    )
}

fn runtime_fields(query: &LaunchModalQuery) -> Element {
    let mut max_len = input()
        .attr("type", "number")
        .attr("id", "launch-max-len")
        .attr("name", "max_model_len")
        .attr("hx-get", with_base_path("/api/v1/vllm/launch-modal"))
        .attr("hx-target", "#launch-modal")
        .attr("hx-swap", "outerHTML")
        .attr("hx-include", "#launch-form")
        .attr("hx-vals", r#"{"recalculate_gpu_util": true}"#)
        .attr("placeholder", "auto");
    if let Some(value) = query
        .max_model_len
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        max_len = max_len.attr("value", value);
    }

    div()
        .class("form-row launch-form-row")
        .child(
            div()
                .class("form-group")
                .child(
                    label()
                        .attr("data-i18n", "ui_vllm_form_quant")
                        .text("Quantization"),
                )
                .child(quantization_select(query.quantization.as_deref())),
        )
        .child(
            div()
                .class("form-group")
                .child(
                    label()
                        .attr("data-i18n", "ui_vllm_form_max_len")
                        .text("Max Model Len"),
                )
                .child(max_len),
        )
}

fn quantization_select(selected: Option<&str>) -> Element {
    let mut select = select()
        .attr("id", "launch-quant")
        .attr("name", "quantization")
        .attr("hx-get", with_base_path("/api/v1/vllm/launch-modal"))
        .attr("hx-target", "#launch-modal")
        .attr("hx-swap", "outerHTML")
        .attr("hx-include", "#launch-form")
        .attr("hx-vals", r#"{"recalculate_gpu_util": true}"#)
        .child(option().attr("value", "").text("auto"));
    for quantization in [
        "awq",
        "gptq",
        "awq_marlin",
        "gptq_marlin",
        "fp8",
        "bitsandbytes",
    ] {
        let mut opt = option().attr("value", quantization).text(quantization);
        if selected == Some(quantization) {
            opt = opt.attr("selected", "selected");
        }
        select = select.child(opt);
    }
    select
}

fn memory_fields(gpu_util: f32, prefix_caching: bool) -> Element {
    let mut prefix = input()
        .attr("type", "checkbox")
        .attr("id", "launch-prefix-caching")
        .attr("name", "prefix_caching")
        .attr("value", "true");
    if prefix_caching {
        prefix = prefix.attr("checked", "checked");
    }

    div()
        .class("form-row launch-form-row")
        .child(
            div()
                .class("form-group")
                .child(
                    label()
                        .attr("data-i18n", "ui_vllm_form_gpu_util")
                        .text("GPU Utilization"),
                )
                .child(
                    input()
                        .attr("type", "number")
                        .attr("id", "launch-gpu-util")
                        .attr("name", "gpu_memory_utilization")
                        .attr("step", "0.05")
                        .attr("min", "0.1")
                        .attr("max", "1.0")
                        .attr("value", format!("{gpu_util:.2}")),
                ),
        )
        .child(
            div().class("form-group form-group-checkbox").child(
                label().class("checkbox-control").child(prefix).child(
                    span()
                        .class("checkbox-copy")
                        .attr("data-i18n", "ui_vllm_form_prefix_caching")
                        .text("Prefix Caching"),
                ),
            ),
        )
}

fn task_fields(query: &LaunchModalQuery) -> Element {
    let selected_task = query.task.as_deref().unwrap_or("");

    let mut task_select = select()
        .attr("id", "launch-task")
        .attr("name", "task")
        .attr("hx-get", with_base_path("/api/v1/vllm/launch-modal"))
        .attr("hx-target", "#launch-modal")
        .attr("hx-swap", "outerHTML")
        .attr("hx-include", "#launch-form");
    // "auto" leaves the runner flags off (vLLM infers from the model);
    // "embed" launches with --runner pooling --convert embed so
    // /v1/embeddings is served for embedding models.
    for (value, text) in [("", "auto"), ("generate", "generate"), ("embed", "embed")] {
        let mut opt = option().attr("value", value).text(text);
        if selected_task == value {
            opt = opt.attr("selected", "selected");
        }
        task_select = task_select.child(opt);
    }

    let mut tool_calling = input()
        .attr("type", "checkbox")
        .attr("id", "launch-enable-tool-calling")
        .attr("name", "enable_tool_calling")
        .attr("value", "true");
    if query.enable_tool_calling.unwrap_or(false) {
        tool_calling = tool_calling.attr("checked", "checked");
    }

    div()
        .class("form-row launch-form-row")
        .child(
            div()
                .class("form-group")
                .child(label().attr("data-i18n", "ui_vllm_form_task").text("Task"))
                .child(task_select),
        )
        .child(
            div().class("form-group form-group-checkbox").child(
                label().class("checkbox-control").child(tool_calling).child(
                    span()
                        .class("checkbox-copy")
                        .attr("data-i18n", "ui_vllm_form_tool_calling")
                        .text("Tool Calling"),
                ),
            ),
        )
}

fn close_launch_button() -> Element {
    button()
        .class("modal-close")
        .attr("type", "button")
        .attr("hx-get", with_base_path("/api/v1/vllm/launch-modal/empty"))
        .attr("hx-target", "#launch-modal")
        .attr("hx-swap", "outerHTML")
        .text("x")
}

fn render_stop_modal(id: &str, model: &str) -> String {
    div()
        .attr("id", "confirm-stop-instance-modal")
        .attr("data-testid", "stop-instance-modal")
        .class("estimates-modal stop-instance-modal open")
        .child(
            button()
                .class("estimates-modal-backdrop")
                .attr("type", "button")
                .attr("hx-get", with_base_path("/api/v1/vllm/stop-modal/empty"))
                .attr("hx-target", "#confirm-stop-instance-modal")
                .attr("hx-swap", "outerHTML"),
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
                                .attr("hx-get", with_base_path("/api/v1/vllm/stop-modal/empty"))
                                .attr("hx-target", "#confirm-stop-instance-modal")
                                .attr("hx-swap", "outerHTML")
                                .child(i().class("fa-solid fa-xmark")),
                        ),
                )
                .child(
                    div()
                        .class("estimates-modal-body")
                        .child(p().text("Are you sure you want to stop this instance?"))
                        .child(div().class("model-to-delete-name").text(model))
                        .child(
                            div()
                                .class("confirm-actions")
                                .child(
                                    button()
                                        .class("button cancel")
                                        .attr("type", "button")
                                        .attr(
                                            "hx-get",
                                            with_base_path("/api/v1/vllm/stop-modal/empty"),
                                        )
                                        .attr("hx-target", "#confirm-stop-instance-modal")
                                        .attr("hx-swap", "outerHTML")
                                        .text("Cancel"),
                                )
                                .child(
                                    button()
                                        .class("button delete")
                                        .attr("type", "button")
                                        .text("Stop Instance")
                                        .attr(
                                            "hx-delete",
                                            with_base_path(&format!("/api/v1/vllm/instances/{id}")),
                                        )
                                        .attr("hx-target", "#confirm-stop-instance-modal")
                                        .attr("hx-swap", "outerHTML"),
                                ),
                        ),
                ),
        )
        .render()
}

fn launch_fit_note(
    model: Option<&crate::routers::models::Model>,
    quantization: &str,
    max_model_len: Option<u32>,
    gpu_util: f32,
    gpu: &crate::routers::gpu::monitor::GpuInfo,
) -> (&'static str, String, &'static str, bool) {
    let Some(model) = model else {
        return (
            "fit-line fit-warn",
            "Select a model to estimate required VRAM.".to_string(),
            "ui_vllm_fit_select_model",
            true,
        );
    };

    let Some(estimate) = find_launch_estimate(model, quantization, max_model_len) else {
        return (
            "fit-line fit-warn",
            "No matching estimate available.".to_string(),
            "ui_vllm_fit_no_estimate",
            true,
        );
    };

    let estimated_model_gb = estimate.weights_gb + (estimate.kv_gb * gpu_util as f64);
    let total_budget_gb = gpu.total_gb * gpu_util as f64;
    let required_gb = estimated_model_gb.max(total_budget_gb);
    let remaining_gb = gpu.free_gb - required_gb;

    if estimated_model_gb > total_budget_gb {
        (
            "fit-line fit-no",
            format!(
                "Won't fit: model needs ~{estimated_model_gb:.2} GB for the selected max length, but gpu memory utilization allows only {total_budget_gb:.2} GB"
            ),
            "ui_vllm_fit_wont_fit_budget",
            true,
        )
    } else if required_gb > gpu.free_gb {
        (
            "fit-line fit-no",
            format!(
                "Won't fit right now: vLLM will reserve ~{required_gb:.2} GB, but only {:.2} GB is free",
                gpu.free_gb
            ),
            "ui_vllm_fit_wont_fit_free",
            true,
        )
    } else if remaining_gb < 2.0 {
        (
            "fit-line fit-warn",
            format!(
                "Tight fit: model needs ~{estimated_model_gb:.2} GB and vLLM will reserve ~{required_gb:.2} GB, leaving {remaining_gb:.2} GB free"
            ),
            "ui_vllm_fit_tight",
            false,
        )
    } else {
        (
            "fit-line fit-ok",
            format!(
                "Should fit: model needs ~{estimated_model_gb:.2} GB and vLLM will reserve ~{required_gb:.2} GB"
            ),
            "ui_vllm_fit_ok",
            false,
        )
    }
}

fn launch_gpu_util(
    model: Option<&crate::routers::models::Model>,
    quantization: &str,
    max_model_len: Option<u32>,
    query: &LaunchModalQuery,
    gpu: &crate::routers::gpu::monitor::GpuInfo,
) -> f32 {
    if let Some(value) = parse_optional_f32(query.gpu_memory_utilization.as_deref())
        && !query.recalculate_gpu_util.unwrap_or(false)
    {
        return value;
    }

    let Some(model) = model else {
        return parse_optional_f32(query.gpu_memory_utilization.as_deref()).unwrap_or(0.90);
    };
    let Some(estimate) = find_launch_estimate(model, quantization, max_model_len) else {
        return parse_optional_f32(query.gpu_memory_utilization.as_deref()).unwrap_or(0.90);
    };

    calculate_minimum_gpu_util(estimate, gpu.total_gb).unwrap_or(0.90)
}

fn calculate_minimum_gpu_util(
    estimate: &crate::routers::models::ModelEstimate,
    total_gpu_gb: f64,
) -> Option<f32> {
    let kv_gb = estimate.kv_gb;
    let weights_gb = estimate.weights_gb;
    let safety_margin_gb = 1.5;

    if !total_gpu_gb.is_finite() || total_gpu_gb <= 0.0 {
        return None;
    }

    let denominator = total_gpu_gb - kv_gb;
    if denominator <= 0.0 {
        return None;
    }

    let raw = (weights_gb + safety_margin_gb) / denominator;
    if !raw.is_finite() || raw <= 0.0 {
        return Some(0.20);
    }

    Some(round_gpu_util_up(raw).clamp(0.20, 1.0) as f32)
}

fn round_gpu_util_up(value: f64) -> f64 {
    (value * 20.0).ceil() / 20.0
}

fn fit_note_icon_class(class: &str) -> &'static str {
    if class.contains("fit-no") {
        "fa-solid fa-circle-xmark"
    } else if class.contains("fit-warn") {
        "fa-solid fa-triangle-exclamation"
    } else {
        "fa-solid fa-circle-check"
    }
}

fn find_launch_estimate<'a>(
    model: &'a crate::routers::models::Model,
    quantization: &str,
    context: Option<u32>,
) -> Option<&'a crate::routers::models::ModelEstimate> {
    let mut candidates: Vec<_> = model.estimates.iter().collect();

    if !quantization.is_empty() {
        let q_mapped = match quantization {
            "awq" | "awq_marlin" => "AWQ",
            "gptq" | "gptq_marlin" => "GPTQ",
            "fp8" => "FP8",
            "bitsandbytes" => "INT8",
            _ => "",
        };
        if !q_mapped.is_empty() {
            candidates.retain(|estimate| estimate.quant.to_string() == q_mapped);
        }
    }

    if let Some(ctx) = context {
        candidates.sort_by_key(|estimate| estimate.context.as_usize());
        return candidates
            .into_iter()
            .find(|estimate| estimate.context.as_usize() >= ctx as usize)
            .or(model
                .estimates
                .iter()
                .max_by_key(|estimate| estimate.context.as_usize()));
    }

    candidates.first().copied()
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
        VllmManagementMode::Mock => "mock".to_string(),
    }
}

fn parse_optional_u32(value: Option<&str>) -> Option<u32> {
    value.and_then(|value| {
        let value = value.trim();
        (!value.is_empty())
            .then(|| value.parse::<u32>().ok())
            .flatten()
    })
}

fn parse_optional_f32(value: Option<&str>) -> Option<f32> {
    value.and_then(|value| {
        let value = value.trim();
        (!value.is_empty())
            .then(|| value.parse::<f32>().ok())
            .flatten()
    })
}
