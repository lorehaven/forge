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
