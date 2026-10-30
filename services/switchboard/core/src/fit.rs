//! How a model sits on the GPU: the registry cards' fit line, the estimates table and the launch form's note. The
//! arithmetic is the old server pages' own, moved here so the browser can redo it as the person edits the form.

use crate::model::{Estimate, Gpu, Model};

/// Less than this much memory left over is a tight fit.
pub const TIGHT_GB: f64 = 2.0;

/// How well something fits, which the pages colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FitTone {
    Ok,
    Warn,
    No,
}

/// The order quantisations are preferred in when the context is the same.
#[must_use]
pub fn rank(quant: &str) -> i32 {
    match quant {
        "FP16" => 100,
        "BF16" => 95,
        "FP8" => 90,
        "INT8" => 80,
        "Q80" => 70,
        "Q6K" => 60,
        "AWQ" => 55,
        "Q5KM" | "GPTQ" => 50,
        "Q50" => 45,
        "Q4KM" => 40,
        "Q40" => 35,
        "Q3KM" => 30,
        "Q2K" => 20,
        _ => 0,
    }
}

/// The estimate that fits in `free_gb` with the longest context (then the best quantisation).
#[must_use]
pub fn best_estimate(estimates: &[Estimate], free_gb: f64) -> Option<&Estimate> {
    estimates
        .iter()
        .filter(|e| e.total_gb <= free_gb)
        .max_by(|a, b| {
            let (a_ctx, b_ctx) = (a.context.as_usize(), b.context.as_usize());
            if a_ctx == b_ctx {
                rank(&a.quant).cmp(&rank(&b.quant))
            } else {
                a_ctx.cmp(&b_ctx)
            }
        })
}

/// The estimate that takes the least memory.
#[must_use]
pub fn minimum_estimate(estimates: &[Estimate]) -> Option<&Estimate> {
    estimates.iter().min_by(|a, b| {
        a.total_gb
            .partial_cmp(&b.total_gb)
            .unwrap_or(std::cmp::Ordering::Equal)
    })
}

/// What a registry card says about the model.
#[derive(Debug, Clone, PartialEq)]
pub enum CardFit {
    /// An estimate fits: the best one, and what is left over.
    Fits {
        context: String,
        quant: String,
        total_gb: f64,
        margin_gb: f64,
        tone: FitTone,
    },
    /// Nothing fits: the least it would take.
    TooBig {
        min_gb: f64,
    },
    NoEstimates,
}

#[must_use]
pub fn card_fit(model: &Model, free_gb: f64) -> CardFit {
    if let Some(best) = best_estimate(&model.estimates, free_gb) {
        let margin = free_gb - best.total_gb;
        return CardFit::Fits {
            context: best.context.to_string(),
            quant: best.quant.clone(),
            total_gb: best.total_gb,
            margin_gb: margin,
            tone: if margin <= TIGHT_GB {
                FitTone::Warn
            } else {
                FitTone::Ok
            },
        };
    }
    minimum_estimate(&model.estimates).map_or(CardFit::NoEstimates, |min| CardFit::TooBig {
        min_gb: min.total_gb,
    })
}

/// A line of the estimates table.
#[derive(Debug, Clone, PartialEq)]
pub struct EstimateRow {
    pub context: String,
    pub quant: String,
    pub total_gb: f64,
    pub margin_gb: f64,
    pub tone: FitTone,
}

/// The estimates the filters leave. `fit` is `all`, `fit` or `nofit`; `context` and `quant` are `all` or a value.
#[must_use]
pub fn estimate_rows(
    model: &Model,
    free_gb: f64,
    fit: &str,
    context: &str,
    quant: &str,
) -> Vec<EstimateRow> {
    model
        .estimates
        .iter()
        .filter(|e| {
            let fits = e.total_gb <= free_gb;
            (fit != "fit" || fits)
                && (fit != "nofit" || !fits)
                && (context == "all" || e.context.to_string() == context)
                && (quant == "all" || e.quant == quant)
        })
        .map(|e| {
            let margin = free_gb - e.total_gb;
            EstimateRow {
                context: e.context.to_string(),
                quant: e.quant.clone(),
                total_gb: e.total_gb,
                margin_gb: margin,
                tone: if e.total_gb > free_gb {
                    FitTone::No
                } else if margin <= TIGHT_GB {
                    FitTone::Warn
                } else {
                    FitTone::Ok
                },
            }
        })
        .collect()
}

/// The distinct contexts (ascending) and quantisations the model has estimates for.
#[must_use]
pub fn filter_options(model: &Model) -> (Vec<String>, Vec<String>) {
    let mut contexts: Vec<String> = model
        .estimates
        .iter()
        .map(|e| e.context.to_string())
        .collect();
    contexts.sort_by_key(|c| c.parse::<usize>().unwrap_or_default());
    contexts.dedup();
    let mut quants: Vec<String> = model.estimates.iter().map(|e| e.quant.clone()).collect();
    quants.sort();
    quants.dedup();
    (contexts, quants)
}

/// The estimate the launch form sizes the model by: for no quantisation override the model's own, else the one the override
/// names; the first at or above the asked-for length, or the longest there is.
#[must_use]
pub fn find_launch_estimate<'a>(
    model: &'a Model,
    quantization: &str,
    context: Option<u32>,
) -> Option<&'a Estimate> {
    let mut candidates: Vec<&Estimate> = model.estimates.iter().collect();
    if quantization.is_empty() {
        // No override: vLLM loads the checkpoint as-is, so size it by the model's own quant (a pre-quantized 4-bit
        // build, say), not by whichever what-if row sorts first.
        let own: Vec<&Estimate> = candidates
            .iter()
            .copied()
            .filter(|e| e.quant == model.quant)
            .collect();
        if !own.is_empty() {
            candidates = own;
        }
    } else {
        let mapped = match quantization {
            "awq" | "awq_marlin" => "AWQ",
            "gptq" | "gptq_marlin" => "GPTQ",
            "fp8" => "FP8",
            "bitsandbytes" => "INT8",
            _ => "",
        };
        if !mapped.is_empty() {
            candidates.retain(|e| e.quant == mapped);
        }
    }
    if let Some(ctx) = context {
        candidates.sort_by_key(|e| e.context.as_usize());
        return candidates
            .into_iter()
            .find(|e| e.context.as_usize() >= ctx as usize)
            .or_else(|| model.estimates.iter().max_by_key(|e| e.context.as_usize()));
    }
    candidates.first().copied()
}

/// The nearest `f32` to `value`, by way of its shortest decimal form (so 0.35 stays 0.35).
fn f64_to_f32(value: f64) -> f32 {
    value.to_string().parse().unwrap_or(0.20)
}

/// Up to the next multiple of 0.05.
#[must_use]
pub fn round_gpu_util_up(value: f64) -> f64 {
    (value * 20.0).ceil() / 20.0
}

/// The least share of the GPU's memory that holds the estimate (weights and a safety margin over what is left after the
/// cache), in steps of 0.05; `None` when the card is unknown or the cache alone is bigger than it.
#[must_use]
pub fn minimum_gpu_util(estimate: &Estimate, total_gpu_gb: f64) -> Option<f32> {
    let safety_margin_gb = 1.5;
    if !total_gpu_gb.is_finite() || total_gpu_gb <= 0.0 {
        return None;
    }
    let denominator = total_gpu_gb - estimate.kv_gb;
    if denominator <= 0.0 {
        return None;
    }
    let raw = (estimate.weights_gb + safety_margin_gb) / denominator;
    if !raw.is_finite() || raw <= 0.0 {
        return Some(0.20);
    }
    Some(f64_to_f32(round_gpu_util_up(raw).clamp(0.20, 1.0)))
}

/// The GPU memory share the form should show: what was typed unless the choice of model, quantisation, length or device
/// changed (`recalculate`), then the least that holds the model (0.90 when that is unknown).
#[must_use]
pub fn launch_gpu_util(
    model: Option<&Model>,
    quantization: &str,
    max_model_len: Option<u32>,
    typed: Option<f32>,
    recalculate: bool,
    gpu: &Gpu,
) -> f32 {
    if let Some(value) = typed
        && !recalculate
    {
        return value;
    }
    let Some(model) = model else {
        return typed.unwrap_or(0.90);
    };
    let Some(estimate) = find_launch_estimate(model, quantization, max_model_len) else {
        return typed.unwrap_or(0.90);
    };
    minimum_gpu_util(estimate, gpu.total_gb).unwrap_or(0.90)
}

/// The launch form's note under the fields: whether the chosen model fits and whether Launch can be pressed.
#[derive(Debug, Clone, PartialEq)]
pub struct LaunchFit {
    pub tone: FitTone,
    /// The translation key; `args` fill its placeholders.
    pub key: &'static str,
    pub args: Vec<(&'static str, String)>,
    pub launch_enabled: bool,
}

impl LaunchFit {
    fn plain(tone: FitTone, key: &'static str, launch_enabled: bool) -> Self {
        Self {
            tone,
            key,
            args: Vec::new(),
            launch_enabled,
        }
    }

    /// Running on the CPU: the card does not come into it.
    #[must_use]
    pub fn cpu() -> Self {
        Self::plain(FitTone::Ok, "ui_vllm_fit_note_cpu", true)
    }
}

/// Whether `model` fits the GPU at `gpu_util` of its memory.
#[must_use]
pub fn launch_fit(
    model: Option<&Model>,
    quantization: &str,
    max_model_len: Option<u32>,
    gpu_util: f32,
    gpu: &Gpu,
) -> LaunchFit {
    let Some(model) = model else {
        return LaunchFit::plain(FitTone::Warn, "ui_vllm_fit_select_model", false);
    };
    let Some(estimate) = find_launch_estimate(model, quantization, max_model_len) else {
        return LaunchFit::plain(FitTone::Warn, "ui_vllm_fit_no_estimate", false);
    };
    let util = f64::from(gpu_util);
    let model_gb = estimate.kv_gb.mul_add(util, estimate.weights_gb);
    let budget_gb = gpu.total_gb * util;
    let required_gb = model_gb.max(budget_gb);
    let remaining_gb = gpu.free_gb - required_gb;
    let args = vec![
        ("model", format!("{model_gb:.2}")),
        ("budget", format!("{budget_gb:.2}")),
        ("required", format!("{required_gb:.2}")),
        ("free", format!("{:.2}", gpu.free_gb)),
        ("remaining", format!("{remaining_gb:.2}")),
    ];
    let (tone, key, enabled) = if model_gb > budget_gb {
        (FitTone::No, "ui_vllm_fit_wont_fit_budget", false)
    } else if required_gb > gpu.free_gb {
        (FitTone::No, "ui_vllm_fit_wont_fit_free", false)
    } else if remaining_gb < TIGHT_GB {
        (FitTone::Warn, "ui_vllm_fit_tight", true)
    } else {
        (FitTone::Ok, "ui_vllm_fit_ok", true)
    };
    LaunchFit {
        tone,
        key,
        args,
        launch_enabled: enabled,
    }
}
