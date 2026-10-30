use super::store::get_store;
use super::types::{Model, ModelEstimate, ModelFilters};
use crate::routers::gpu::get_gpu_info;
use crate::routers::gpu::monitor::GpuInfo;
use quench_http::prelude::{Json, post};

#[post("/api/v1/models/list")]
pub async fn handle_list(Json(body): Json<ModelFilters>) -> Json<Vec<Model>> {
    let gpu = get_gpu_info().unwrap_or_default();
    let mut models = get_store().get_all_models().await;

    apply_filters(&mut models, &body, &gpu);

    Json(models)
}

pub fn apply_filters(models: &mut Vec<Model>, filters: &ModelFilters, gpu: &GpuInfo) {
    // Defaults to HF when unspecified; "all" keeps every source (the launch form lists them together).
    let source = filters
        .source
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or("hf");
    let source_lower = source.to_lowercase();
    if source_lower != "all" {
        models.retain(|m| m.source.to_lowercase() == source_lower);
    }

    if let Some(search) = &filters.search {
        let search_lower = search.to_lowercase();
        if !search_lower.is_empty() {
            models.retain(|m| m.name.to_lowercase().contains(&search_lower));
        }
    }

    if let Some(quant_str) = &filters.quant
        && quant_str != "ALL"
        && !quant_str.is_empty()
    {
        models.retain(|m| {
            // Debug format, or a known alias.
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

    if let Some(context_val) = &filters.context {
        let context_num = match context_val {
            quench_serde::Value::UInt(_)
            | quench_serde::Value::Int(_)
            | quench_serde::Value::Float(_) => {
                usize::try_from(context_val.as_u64().unwrap_or(0)).unwrap_or_default()
            }
            quench_serde::Value::String(s) => s.parse::<usize>().unwrap_or(0),
            _ => 0,
        };

        if context_num != 0 {
            models.retain(|m| m.context.as_usize() >= context_num);
        }
    }

    if let Some(vllm_only) = filters.vllm_only
        && vllm_only
    {
        models.retain(|m| m.vllm_supported);
    }

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
                let vram_a = best_a.map_or(1000.0, |e| e.total_gb);
                let vram_b = best_b.map_or(1000.0, |e| e.total_gb);
                vram_a
                    .partial_cmp(&vram_b)
                    .unwrap_or(std::cmp::Ordering::Equal)
            }),
            "vram_desc" => models.sort_by(|a, b| {
                let best_a = find_best_estimate(&a.estimates, gpu.free_gb);
                let best_b = find_best_estimate(&b.estimates, gpu.free_gb);
                let vram_a = best_a.map_or(1000.0, |e| e.total_gb);
                let vram_b = best_b.map_or(1000.0, |e| e.total_gb);
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
            if a_ctx == b_ctx {
                a.quant.rank().cmp(&b.quant.rank())
            } else {
                a_ctx.cmp(&b_ctx)
            }
        })
}
