use super::types::VllmInstance;
use crate::routers::models::mod_impl::is_admin;
use crate::routers::vllm::engine::VllmEngine;
use actix_web::{HttpResponse, Responder, get, web};
use quench_auth::prelude::JwtConfig;
use quench_starter::prelude::with_base_path;
use quench_web::prelude::*;
use std::sync::Arc;

#[get("/list")]
pub async fn list_instances_canonical(engine: web::Data<Arc<dyn VllmEngine>>) -> impl Responder {
    list_instances_impl(engine).await
}

#[get("/instances")]
pub async fn list_instances_alias(engine: web::Data<Arc<dyn VllmEngine>>) -> impl Responder {
    list_instances_impl(engine).await
}

async fn list_instances_impl(engine: web::Data<Arc<dyn VllmEngine>>) -> impl Responder {
    match engine.list_instances().await {
        Ok(list) => HttpResponse::Ok().json(list),
        Err(err) => {
            tracing::error!("Failed to list vLLM instances: {}", err);
            HttpResponse::InternalServerError().body(err)
        }
    }
}

#[get("/grid")]
pub async fn handle_grid(
    req: actix_web::HttpRequest,
    config: web::Data<JwtConfig>,
    engine: web::Data<Arc<dyn VllmEngine>>,
) -> impl Responder {
    let admin = is_admin(&req, &config);
    match engine.list_instances().await {
        Ok(instances) => {
            let html = render_instances_grid(instances, admin);
            HttpResponse::Ok().content_type("text/html").body(html)
        }
        Err(err) => {
            tracing::error!("Failed to list vLLM instances for grid: {}", err);
            HttpResponse::InternalServerError().body(err)
        }
    }
}

pub fn render_instances_grid(instances: Vec<VllmInstance>, is_admin: bool) -> String {
    if instances.is_empty() {
        return instances_grid_shell()
            .child(div().class("empty").text("No running instances"))
            .render();
    }

    let mut grid = instances_grid_shell();

    for instance in instances {
        let status_class = match instance.status.as_str() {
            "failed" => "status-failed",
            "starting" => "status-starting",
            "terminating" => "status-terminating",
            _ => "status-running",
        };

        let fit_class = match instance.status.as_str() {
            "failed" => "fit-no",
            "starting" => "fit-warn",
            "terminating" => "fit-warn",
            _ => "fit-ok",
        };

        let mut header = div()
            .class("card-header")
            .child(div().class("card-title").text(&instance.model));

        if is_admin && instance.status != "failed" && instance.status != "terminating" {
            header = header.child(
                button()
                    .class("card-delete")
                    .attr("type", "button")
                    .attr("title", "Stop instance")
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

        let card = div()
            .class("card")
            .child(header)
            .child(
                div()
                    .class("card-meta")
                    .child(
                        div()
                            .class("meta-item")
                            .child(span().text("ID: "))
                            .child(span().class("instance-id").text(&instance.id)),
                    )
                    .child(
                        div()
                            .class("meta-item")
                            .child(span().text("Namespace: "))
                            .child(span().class("instance-namespace").text(&instance.namespace)),
                    )
                    .child(
                        div()
                            .class("meta-item")
                            .child(span().text("Endpoint: "))
                            .child(
                                span()
                                    .class("instance-endpoint")
                                    .text(format!("{}:{}", instance.host, instance.port)),
                            ),
                    )
                    .child(
                        div()
                            .class("meta-item")
                            .child(span().text("Status: "))
                            .child(
                                span()
                                    .class(format!("instance-status {}", status_class))
                                    .text(&instance.status),
                            ),
                    )
                    .child(
                        div()
                            .class("meta-item")
                            .child(span().text("Started: "))
                            .child(
                                span().class("instance-started").text(
                                    instance.started_at.format("%Y-%m-%d %H:%M:%S").to_string(),
                                ),
                            ),
                    ),
            )
            .child(
                div().class("card-fit").child(
                    div()
                        .class(format!("fit-line {}", fit_class))
                        .child(span().text("Quant: "))
                        .child(
                            span().text(
                                instance
                                    .quantization
                                    .as_deref()
                                    .unwrap_or("auto")
                                    .to_string(),
                            ),
                        )
                        .child(span().text(" | "))
                        .child(span().text("GPU Util: "))
                        .child(span().text(format!(
                            "{:.2}",
                            instance.gpu_memory_utilization.unwrap_or(0.9)
                        ))),
                ),
            )
            .child({
                let mut diag = div().class("instance-diagnostics");
                let mut has_diag = false;

                if let Some(err) = &instance.last_error {
                    has_diag = true;
                    diag = diag.child(div().class("instance-error").text(err));
                }

                if let Some(log) = &instance.log_path {
                    has_diag = true;
                    diag = diag.child(
                        div()
                            .class("instance-log-path")
                            .attr("title", format!("log: {}", log))
                            .text(format!("log: {}", log)),
                    );
                }

                if !has_diag {
                    diag = diag.attr("style", "display: none;");
                }

                diag
            });

        grid = grid.child(card);
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
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}
