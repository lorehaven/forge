//! Unit tests for `routers/models/types.rs`.

use serde_json::json;
use switchboard_service::routers::models::types::*;

#[test]
fn test_context_serialization_roundtrip() {
    let contexts = vec![
        (Context::Size512, json!(512)),
        (Context::Size1024, json!(1024)),
        (Context::ALL, json!("ALL")),
    ];

    for (ctx, expected_json) in contexts {
        // Test serialization
        let serialized = serde_json::to_value(ctx).unwrap();
        assert_eq!(serialized, expected_json);

        // Test deserialization from the serialized value
        let deserialized: Context = serde_json::from_value(serialized).unwrap();
        assert_eq!(deserialized, ctx);
    }
}

#[test]
fn test_context_deserialization_from_string() {
    let json_str = json!("4096");
    let deserialized: Context = serde_json::from_value(json_str).unwrap();
    assert_eq!(deserialized, Context::Size4096);
}

#[test]
fn test_model_deserialization_with_numeric_context() {
    let model_json = json!({
        "source": "HF",
        "name": "test-model",
        "path": "/path/to/model",
        "architecture": "LlamaForCausalLM",
        "vllm_supported": true,
        "quant": "FP16",
        "context": 4096,
        "layers": 32,
        "hidden_size": 4096,
        "params_billion": 7.0,
        "estimates": []
    });

    let model: Model = serde_json::from_value(model_json).unwrap();
    assert_eq!(model.context, Context::Size4096);
    assert_eq!(model.name, "test-model");
}

#[test]
fn context_invalid_type_is_rejected() {
    let err = serde_json::from_value::<Context>(json!(true)).unwrap_err();
    assert!(err.to_string().contains("invalid context type"));
}

#[test]
fn context_invalid_value_is_rejected() {
    let err = serde_json::from_value::<Context>(json!("999")).unwrap_err();
    assert!(err.to_string().contains("invalid context value"));
}

#[test]
fn context_as_usize_matches_the_advertised_size() {
    assert_eq!(Context::Size512.as_usize(), 512);
    assert_eq!(Context::Size131072.as_usize(), 131072);
    assert_eq!(Context::ALL.as_usize(), 131072);
}

#[test]
fn context_display_matches_serialization() {
    for ctx in Context::ALL_VALUES {
        assert_eq!(ctx.to_string(), ctx.as_usize().to_string());
    }
    assert_eq!(Context::ALL.to_string(), "ALL");
}

#[test]
fn quant_aliases_deserialize_to_the_same_variant() {
    let dashed: Quant = serde_json::from_value(json!("Q8_0")).unwrap();
    let compact: Quant = serde_json::from_value(json!("Q80")).unwrap();
    assert_eq!(dashed, compact);
    assert_eq!(dashed, Quant::Q80);

    let lower: Quant = serde_json::from_value(json!("fp16")).unwrap();
    assert_eq!(lower, Quant::FP16);
}

#[test]
fn quant_bytes_per_weight_covers_every_variant() {
    assert_eq!(Quant::FP16.bytes_per_weight(), 2.0);
    assert_eq!(Quant::Q2K.bytes_per_weight(), 0.35);
    assert_eq!(Quant::ALL.bytes_per_weight(), 0.0);
}

#[test]
fn quant_rank_orders_higher_fidelity_above_lower() {
    assert!(Quant::FP16.rank() > Quant::Q80.rank());
    assert!(Quant::Q80.rank() > Quant::Q2K.rank());
    assert_eq!(Quant::ALL.rank(), 0);
}

#[test]
fn quant_display_uses_debug_formatting() {
    assert_eq!(Quant::FP16.to_string(), "FP16");
    assert_eq!(format!("{}", Quant::Q4KM), "Q4KM");
}

#[test]
fn model_filters_accept_type_alias_for_source() {
    let filters: ModelFilters = serde_json::from_value(json!({ "type": "GGUF" })).unwrap();
    assert_eq!(filters.source.as_deref(), Some("GGUF"));
    assert!(filters.search.is_none());
}

#[test]
fn model_filters_accept_numeric_context() {
    let filters: ModelFilters = serde_json::from_value(json!({ "context": 4096 })).unwrap();
    assert_eq!(filters.context, Some(json!(4096)));
}

#[test]
fn delete_model_request_round_trips() {
    let request: DeleteModelRequest =
        serde_json::from_value(json!({ "path": "/mnt/dev/quantized/model.gguf" })).unwrap();
    assert_eq!(request.path, "/mnt/dev/quantized/model.gguf");
}

#[test]
fn running_model_serializes_every_field() {
    let running = RunningModel {
        id: "id-1".to_string(),
        model: "llama".to_string(),
        endpoint: "http://localhost:8000".to_string(),
        status: "running".to_string(),
    };

    let value = serde_json::to_value(&running).unwrap();
    assert_eq!(value["id"], "id-1");
    assert_eq!(value["endpoint"], "http://localhost:8000");
}

#[test]
fn model_estimate_round_trips_through_json() {
    let estimate = ModelEstimate {
        quant: Quant::Q4KM,
        context: Context::Size8192,
        weights_gb: 4.5,
        kv_gb: 1.2,
        total_gb: 5.7,
    };

    let value = serde_json::to_value(&estimate).unwrap();
    let back: ModelEstimate = serde_json::from_value(value).unwrap();
    assert_eq!(back.quant, Quant::Q4KM);
    assert_eq!(back.context, Context::Size8192);
    assert_eq!(back.total_gb, 5.7);
}
