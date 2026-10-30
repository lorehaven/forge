use quench_serde::json;
use switchboard_core::{Capabilities, Context, Gpu, Instance, Model};

#[quench_test::test]
fn the_gpu_answer_reads_and_an_empty_one_is_absent() {
    let gpu: Gpu =
        json::from_str(r#"{"name":"card1","total_gb":32,"used_gb":8.5,"free_gb":23.5}"#).unwrap();
    assert!(gpu.present());
    assert!((gpu.free_gb - 23.5).abs() < 1e-9);
    assert!((gpu.total_gb - 32.0).abs() < 1e-9);
    assert!(!Gpu::default().present());
}

#[quench_test::test]
fn capabilities_read() {
    let caps: Capabilities = json::from_str(
        r#"{"models_dashboard":true,"vllm_management":false,"can_launch":true,"can_stop":false,"can_delete_model":true,"namespace":"native"}"#,
    )
    .unwrap();
    assert!(caps.models_dashboard && caps.can_launch && caps.can_delete_model);
    assert!(!caps.vllm_management && !caps.can_stop);
    assert_eq!(caps.namespace, "native");
}

#[quench_test::test]
fn a_context_is_a_number_or_all() {
    let all: Context = json::from_str(r#""ALL""#).unwrap();
    assert_eq!((all.0, all.to_string().as_str()), (0, "ALL"));
    assert_eq!(all.as_usize(), 131_072);
    let size: Context = json::from_str("4096").unwrap();
    assert_eq!((size.0, size.to_string().as_str()), (4096, "4096"));
    let text: Context = json::from_str(r#""8192""#).unwrap();
    assert_eq!(text.0, 8192);
    assert!(json::from_str::<Context>(r#""huge""#).is_err());
    assert_eq!(json::to_string(&size), "4096");
    assert_eq!(json::to_string(&all), r#""ALL""#);
}

#[quench_test::test]
fn a_model_reads_with_its_estimates() {
    let model: Model = json::from_str(
        r#"{"source":"HF","name":"llama","path":"/m/llama","architecture":null,"vllm_supported":true,
        "quant":"FP16","context":"ALL","layers":32,"hidden_size":4096,"params_billion":7.0,
        "estimates":[{"quant":"FP16","context":4096,"weights_gb":14.0,"kv_gb":1.0,"total_gb":15.5}]}"#,
    )
    .unwrap();
    assert!(model.vllm_supported);
    assert_eq!(model.estimates.len(), 1);
    assert_eq!(model.estimates[0].context.0, 4096);
}

const INSTANCE: &str = r#"{"id":"vllm-1","namespace":"native","model":"llama","host":"0.0.0.0","port":8000,
    "quantization":null,"max_model_len":null,"gpu_memory_utilization":0.9,"enable_prefix_caching":false,
    "enable_tool_calling":false,"started_at":"2026-10-29T10:01:02.345Z","status":"running","log_path":null,"last_error":null}"#;

#[quench_test::test]
fn an_instance_reads_without_the_optional_fields() {
    let i: Instance = json::from_str(INSTANCE).unwrap();
    assert_eq!(i.endpoint(), "0.0.0.0:8000");
    assert_eq!(i.started(), "2026-10-29 10:01:02");
    assert_eq!(i.quant_text(), "auto");
    assert_eq!(i.gpu_util_text(), "0.90");
    assert!(!i.is_cpu());
    assert!(i.stoppable());
}

#[quench_test::test]
fn an_instance_knows_its_device_and_state() {
    use switchboard_core::model::Tone;
    let mut i: Instance = json::from_str(INSTANCE).unwrap();
    i.device = Some(" CPU ".into());
    assert!(i.is_cpu());
    i.device = Some("gpu".into());
    assert!(!i.is_cpu());
    for (status, tone, stoppable) in [
        ("running", Tone::Ok, true),
        ("starting", Tone::Warn, true),
        ("terminating", Tone::Warn, false),
        ("failed", Tone::Failed, false),
        ("completed", Tone::Failed, true),
        ("pending", Tone::Ok, true),
    ] {
        i.status = status.into();
        assert_eq!((i.tone(), i.stoppable()), (tone, stoppable), "{status}");
    }
    i.quantization = Some("  ".into());
    assert_eq!(i.quant_text(), "auto");
    i.quantization = Some("awq".into());
    assert_eq!(i.quant_text(), "awq");
}
