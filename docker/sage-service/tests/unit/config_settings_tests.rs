//! Unit tests for `config/settings.rs`.

use sage_service::config::settings::*;

#[test]
fn test_parse_default_models_json() {
    // Array of models
    let json_arr = r#"[
        {"name": "Qwen/Qwen2.5-0.5B-Instruct", "gpu_utilization": 0.20, "context_len": 32768},
        {"name": "llama3.1:8b", "gpu_utilization": 0.90}
    ]"#;
    let list = DefaultModel::parse_list(json_arr);
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].name, "Qwen/Qwen2.5-0.5B-Instruct");
    assert_eq!(list[0].gpu_memory_utilization, Some(0.20));
    assert_eq!(list[0].max_model_len, Some(32768));
    assert_eq!(list[1].name, "llama3.1:8b");
    assert_eq!(list[1].gpu_memory_utilization, Some(0.90));
    assert_eq!(list[1].max_model_len, None);

    assert_eq!(list[0].task, None);

    // Single JSON object
    let json_obj = r#"{"name": "qwen2.5-coder:7b", "context_len": 4096}"#;
    let list2 = DefaultModel::parse_list(json_obj);
    assert_eq!(list2.len(), 1);
    assert_eq!(list2[0].name, "qwen2.5-coder:7b");
    assert_eq!(list2[0].gpu_memory_utilization, None);
    assert_eq!(list2[0].max_model_len, Some(4096));

    // Embedding model with an explicit task
    let embed_json = r#"{"name": "Qwen/Qwen3-Embedding-0.6B", "gpu_utilization": 0.12, "context_len": 8192, "task": "embed"}"#;
    let list_embed = DefaultModel::parse_list(embed_json);
    assert_eq!(list_embed.len(), 1);
    assert_eq!(list_embed[0].task.as_deref(), Some("embed"));
    assert_eq!(list_embed[0].quantization, None);

    // Quantized chat model
    let quant_json = r#"{"name": "Qwen/Qwen2.5-Coder-7B-Instruct-AWQ", "quant": "awq", "gpu_utilization": 0.55}"#;
    let list_quant = DefaultModel::parse_list(quant_json);
    assert_eq!(list_quant.len(), 1);
    assert_eq!(list_quant[0].quantization.as_deref(), Some("awq"));
    assert_eq!(list_quant[0].dtype, None);

    // Model with an explicit dtype (e.g. GPUs/models that fail on bfloat16)
    let dtype_json = r#"{"name": "mistralai/Mistral-7B-Instruct-v0.3", "dtype": "float16"}"#;
    let list_dtype = DefaultModel::parse_list(dtype_json);
    assert_eq!(list_dtype.len(), 1);
    assert_eq!(list_dtype[0].dtype.as_deref(), Some("float16"));

    // Vision model with a multimodal limit; the value is the exact string
    // shape used in .env (escaped inner quotes) and must survive verbatim.
    let vl_json = r#"{"name": "Qwen/Qwen3-VL-2B-Instruct-FP8", "limit_mm": "{\"image\": 2}"}"#;
    let list_vl = DefaultModel::parse_list(vl_json);
    assert_eq!(list_vl.len(), 1);
    assert_eq!(
        list_vl[0].limit_mm_per_prompt.as_deref(),
        Some(r#"{"image": 2}"#)
    );

    // Backwards compatibility with raw string name
    let raw_str = "qwen2.5-coder:7b";
    let list3 = DefaultModel::parse_list(raw_str);
    assert_eq!(list3.len(), 1);
    assert_eq!(list3[0].name, "qwen2.5-coder:7b");
    assert_eq!(list3[0].gpu_memory_utilization, None);
    assert_eq!(list3[0].max_model_len, None);
    assert!(!list3[0].enable_tool_calling);
}
