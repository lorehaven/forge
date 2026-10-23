use super::types::VllmInstance;
use crate::routers::models::mod_impl::{OptionalClaims, can};
use crate::routers::vllm::engine::VllmEngine;
use quench_auth::domain::jwt::JwtConfig;
use quench_http::prelude::{Inject, IntoResponse, Json, Response, get};
use quench_starter::common::routes::with_base_path;
use quench_web::prelude::*;
use quench_web_components::containers::empty_state;
use std::fmt::Write as _;
use std::sync::Arc;

#[get("/api/v1/vllm/list")]
pub async fn list_instances_canonical(Inject(engine): Inject<Arc<dyn VllmEngine>>) -> Response {
    list_instances_impl(engine).await
}

#[get("/api/v1/vllm/instances")]
pub async fn list_instances_alias(Inject(engine): Inject<Arc<dyn VllmEngine>>) -> Response {
    list_instances_impl(engine).await
}

async fn list_instances_impl(engine: Arc<Arc<dyn VllmEngine>>) -> Response {
    match engine.list_instances().await {
        Ok(list) => Json(list).into_response(),
        Err(err) => {
            quench_log::error!("Failed to list vLLM instances: {}", err);
            Response::text(
                quench_http::http::StatusCode::INTERNAL_SERVER_ERROR,
                "api_error_vllm_list_failed",
            )
        }
    }
}

#[get("/api/v1/vllm/grid")]
pub async fn handle_grid(
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
    Inject(engine): Inject<Arc<dyn VllmEngine>>,
) -> Response {
    let can_stop = can(claims.as_ref(), &config, "stop");
    match engine.list_instances().await {
        Ok(instances) => {
            let html = render_instances_grid(&instances, can_stop);
            Response::html(quench_http::http::StatusCode::OK, html)
        }
        Err(err) => {
            quench_log::error!("Failed to list vLLM instances for grid: {}", err);
            Response::text(
                quench_http::http::StatusCode::INTERNAL_SERVER_ERROR,
                "api_error_vllm_list_failed",
            )
        }
    }
}

/// A `Label: value` line of a card's metadata; `value` is already an element.
fn meta_item(label_key: &str, label: &str, value: Element) -> Element {
    div()
        .class("meta-item")
        .child(span().attr("data-i18n", label_key).text(label))
        .child(span().text(": "))
        .child(value)
}

/// `" | Label: value"`, as it joins the fit line.
fn fit_part(label_key: &str, label: &str, value: String) -> [Element; 4] {
    [
        span().text(" | "),
        span().attr("data-i18n", label_key).text(label),
        span().text(": "),
        span().text(value),
    ]
}

/// "cpu" (case-insensitive) => CPU; anything else, including None, is a GPU launch (the historical
/// default).
fn is_cpu(instance: &VllmInstance) -> bool {
    instance
        .device
        .as_deref()
        .is_some_and(|d| d.trim().eq_ignore_ascii_case("cpu"))
}

/// The model name, where it runs, and (while it can be stopped) the stop button.
fn instance_header(instance: &VllmInstance, can_stop: bool) -> Element {
    let (device_label, device_i18n, device_class) = if is_cpu(instance) {
        ("CPU", "ui_vllm_device_cpu", "badge badge-device badge-cpu")
    } else {
        ("GPU", "ui_vllm_device_gpu", "badge badge-device badge-gpu")
    };

    let mut header = div()
        .class("card-header")
        .child(div().class("card-title").text(&instance.model))
        .child(
            span()
                .class(device_class)
                .attr("data-i18n", device_i18n)
                .attr("title", format!("Running on {device_label}"))
                .text(device_label),
        );

    if can_stop && instance.status != "failed" && instance.status != "terminating" {
        header = header.child(
            button()
                .class("card-delete")
                .attr("type", "button")
                .attr("title", "Stop instance")
                .attr("data-i18n-title", "ui_vllm_stop_tooltip")
                .attr(
                    "hx-get",
                    format!(
                        "{}?id={}&model={}",
                        with_base_path("/api/v1/vllm/stop-modal"),
                        encode_query_component(&instance.id),
                        encode_query_component(&instance.model)
                    ),
                )
                .attr("hx-target", "#confirm-stop-instance-modal")
                .attr("hx-swap", "outerHTML")
                .child(i().class("fa-solid fa-power-off")),
        );
    }
    header
}

/// Id, namespace, endpoint, status and start time.
fn instance_meta(instance: &VllmInstance) -> Element {
    let status_class = match instance.status.as_str() {
        "failed" | "Failed" | "completed" | "interrupted" => "status-failed",
        "starting" => "status-starting",
        "terminating" => "status-terminating",
        _ => "status-running",
    };

    div()
        .class("card-meta")
        .child(meta_item(
            "ui_vllm_meta_id",
            "ID",
            span().class("instance-id").text(&instance.id),
        ))
        .child(meta_item(
            "ui_vllm_meta_namespace",
            "Namespace",
            span().class("instance-namespace").text(&instance.namespace),
        ))
        .child(meta_item(
            "ui_vllm_meta_endpoint",
            "Endpoint",
            span()
                .class("instance-endpoint")
                .text(format!("{}:{}", instance.host, instance.port)),
        ))
        .child(meta_item(
            "ui_vllm_meta_status",
            "Status",
            span()
                .class(format!("instance-status {status_class}"))
                .attr("data-i18n", format!("ui_vllm_status_{}", instance.status))
                .text(&instance.status),
        ))
        .child(meta_item(
            "ui_vllm_meta_started",
            "Started",
            span()
                .class("instance-started")
                .text(instance.started_at.format("%Y-%m-%d %H:%M:%S")),
        ))
}

/// Quantisation, dtype, multimodal limit, and the device (or its GPU utilisation).
fn instance_fit_line(instance: &VllmInstance) -> Element {
    let fit_class = match instance.status.as_str() {
        "failed" | "Failed" | "completed" | "interrupted" => "fit-no",
        "starting" | "terminating" => "fit-warn",
        _ => "fit-ok",
    };
    let quant = instance
        .quantization
        .as_deref()
        .map(str::trim)
        .filter(|q| !q.is_empty())
        .unwrap_or("auto")
        .to_string();

    let mut fit_line = div()
        .class(format!("fit-line {fit_class}"))
        .child(
            span()
                .attr("data-i18n", "ui_models_card_quant")
                .text("Quant"),
        )
        .child(span().text(": "))
        .child(span().text(quant));
    if let Some(dtype) = instance.dtype.as_deref() {
        for part in fit_part("ui_vllm_form_dtype", "Dtype", dtype.to_string()) {
            fit_line = fit_line.child(part);
        }
    }
    if let Some(limit) = instance.limit_mm_per_prompt.as_deref() {
        for part in fit_part(
            "ui_vllm_form_limit_mm",
            "Multimodal Limit",
            limit.to_string(),
        ) {
            fit_line = fit_line.child(part);
        }
    }
    // GPU utilization is meaningless for a CPU instance - the badge in the header already says
    // where it runs.
    let (key, label, value) = if is_cpu(instance) {
        ("ui_vllm_meta_device", "Device", "CPU".to_string())
    } else {
        (
            "ui_vllm_meta_gpu_util",
            "GPU Util",
            format!("{:.2}", instance.gpu_memory_utilization.unwrap_or(0.9)),
        )
    };
    for part in fit_part(key, label, value) {
        fit_line = fit_line.child(part);
    }
    fit_line
}

/// The last error and the log path; hidden when there is neither.
fn instance_diagnostics(instance: &VllmInstance) -> Element {
    let mut diag = div().class("instance-diagnostics");

    if let Some(err) = &instance.last_error {
        diag = diag.child(div().class("instance-error").text(err));
    }

    if let Some(log) = &instance.log_path {
        diag = diag.child(
            div()
                .class("instance-log-path")
                .attr("title", format!("log: {log}"))
                .text(format!("log: {log}")),
        );
    }

    if instance.last_error.is_none() && instance.log_path.is_none() {
        diag = diag.attr("style", "display: none;");
    }

    diag
}

#[must_use]
pub fn render_instances_grid(instances: &[VllmInstance], can_stop: bool) -> String {
    if instances.is_empty() {
        return instances_grid_shell()
            .child(empty_state("ui_vllm_no_instances").text("No running instances"))
            .render();
    }

    let mut grid = instances_grid_shell();

    for instance in instances {
        grid = grid.child(
            div()
                .class("card")
                .child(instance_header(instance, can_stop))
                .child(instance_meta(instance))
                .child(div().class("card-fit").child(instance_fit_line(instance)))
                .child(instance_diagnostics(instance)),
        );
    }

    grid.render()
}

fn instances_grid_shell() -> Element {
    div()
        .attr("id", "vllm-instances-grid")
        .attr("name", "vllm-instances-grid")
        .attr("sse-swap", "vllm-instances")
        .attr("hx-swap", "outerHTML")
        .class("vllm-instances-grid grid")
}

fn encode_query_component(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            _ => {
                let _ = write!(encoded, "%{byte:02X}");
            }
        }
    }
    encoded
}
