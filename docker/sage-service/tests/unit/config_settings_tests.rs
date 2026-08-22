//! Unit tests for `config/settings.rs`.

use crate::env_support::env_lock;
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

#[test]
fn parse_list_returns_empty_for_blank_input() {
    assert!(DefaultModel::parse_list("").is_empty());
    assert!(DefaultModel::parse_list("   ").is_empty());
}

#[test]
fn parse_list_returns_empty_for_malformed_json_that_looks_like_json() {
    // Starts with `{`/`[`, so the raw-string fallback doesn't apply, and
    // it isn't valid JSON either - the "give up" branch.
    assert!(DefaultModel::parse_list("{not valid json").is_empty());
    assert!(DefaultModel::parse_list("[{\"name\":}]").is_empty());
}

#[test]
fn is_model_supported_matches_glob_style_patterns_case_insensitively() {
    let config = sample_config(vec![
        "qwen*".to_string(),
        "*-instruct".to_string(),
        "llama?".to_string(),
    ]);

    assert!(config.is_model_supported("qwen2.5-coder:7b"));
    assert!(config.is_model_supported("QWEN2.5-CODER"));
    assert!(config.is_model_supported("mistral-instruct"));
    assert!(config.is_model_supported("llama3"));
    assert!(!config.is_model_supported("llama33"));
    assert!(!config.is_model_supported("unrelated-model"));
}

#[test]
fn is_model_supported_is_false_with_no_configured_patterns() {
    let config = sample_config(vec![]);
    assert!(!config.is_model_supported("anything"));
}

fn sample_config(supported_models: Vec<String>) -> SageConfig {
    let value = serde_json::json!({
        "system_prompt": "test",
        "default_models": [],
        "supported_models": supported_models,
        "default_search_provider": "duckduckgo",
        "available_search_providers": [],
        "capability_profile": {
            "name": "web_assistant",
            "description": "test",
            "enabled_tools": [],
            "default_timeout_secs": 60,
            "tool_configs": {}
        },
        "stop_models_on_shutdown": false
    });
    serde_json::from_value(value).expect("valid SageConfig shape")
}

#[test]
fn load_uses_defaults_when_nothing_is_configured() {
    let _guard = env_lock().lock().unwrap_or_else(|p| p.into_inner());
    for key in [
        "SAGE_SYSTEM_PROMPT_PATH",
        "SAGE_CAPABILITY_PROFILE",
        "SAGE_DEFAULT_MODELS",
        "SAGE_SUPPORTED_MODELS",
        "SAGE_SEARCH_PROVIDER",
        "SAGE_STOP_MODELS_ON_SHUTDOWN",
        "BRAVE_SEARCH_API_KEY",
        "SERPAPI_API_KEY",
    ] {
        unsafe { std::env::remove_var(key) };
    }

    let config = SageConfig::load();

    assert_eq!(config.capability_profile.name, "web_assistant");
    assert_eq!(config.default_search_provider, "duckduckgo");
    assert!(!config.stop_models_on_shutdown);
    assert_eq!(config.default_models.len(), 1);
    assert_eq!(config.default_models[0].name, "qwen2.5-coder:7b");
    assert_eq!(
        config.available_search_providers,
        vec!["duckduckgo".to_string(), "searxng".to_string()]
    );
}

#[test]
fn load_falls_back_to_web_assistant_for_an_unknown_capability_profile() {
    let _guard = env_lock().lock().unwrap_or_else(|p| p.into_inner());
    unsafe { std::env::set_var("SAGE_CAPABILITY_PROFILE", "not-a-real-profile") };

    let config = SageConfig::load();
    assert_eq!(config.capability_profile.name, "web_assistant");

    unsafe { std::env::remove_var("SAGE_CAPABILITY_PROFILE") };
}

#[test]
fn load_picks_up_search_provider_api_keys() {
    let _guard = env_lock().lock().unwrap_or_else(|p| p.into_inner());
    unsafe { std::env::set_var("BRAVE_SEARCH_API_KEY", "key") };
    unsafe { std::env::set_var("SERPAPI_API_KEY", "key") };

    let config = SageConfig::load();
    assert_eq!(
        config.available_search_providers,
        vec![
            "brave".to_string(),
            "duckduckgo".to_string(),
            "searxng".to_string(),
            "serpapi".to_string(),
        ]
    );

    unsafe { std::env::remove_var("BRAVE_SEARCH_API_KEY") };
    unsafe { std::env::remove_var("SERPAPI_API_KEY") };
}

#[test]
fn load_stop_models_on_shutdown_and_supported_models_read_their_env_vars() {
    let _guard = env_lock().lock().unwrap_or_else(|p| p.into_inner());
    unsafe { std::env::set_var("SAGE_STOP_MODELS_ON_SHUTDOWN", "true") };
    unsafe { std::env::set_var("SAGE_SUPPORTED_MODELS", "foo, bar , ,baz") };

    let config = SageConfig::load();
    assert!(config.stop_models_on_shutdown);
    assert_eq!(
        config.supported_models,
        vec!["foo".to_string(), "bar".to_string(), "baz".to_string()]
    );

    unsafe { std::env::remove_var("SAGE_STOP_MODELS_ON_SHUTDOWN") };
    unsafe { std::env::remove_var("SAGE_SUPPORTED_MODELS") };
}
