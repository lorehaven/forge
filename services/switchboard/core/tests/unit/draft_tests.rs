use switchboard_core::draft::{GGUF_QUANTS, HF_QUANTS};
use switchboard_core::{DraftError, FilterDraft, LaunchDraft};

#[quench_test::test]
fn the_filters_start_as_the_old_form_did() {
    let body = FilterDraft::default().body();
    assert_eq!(
        (
            body.source.as_str(),
            body.sort.as_str(),
            body.quant.as_str()
        ),
        ("hf", "name_asc", "ALL")
    );
    assert_eq!(body.context, "0");
    assert!(!body.vllm_only);
}

#[quench_test::test]
fn the_quants_follow_the_source() {
    let mut f = FilterDraft::default();
    assert_eq!(f.quants(), &HF_QUANTS);
    f.source = "gguf".into();
    assert_eq!(f.quants(), &GGUF_QUANTS);
    f.search = "  llama ".into();
    assert_eq!(f.body().search, "llama");
}

#[quench_test::test]
fn a_launch_needs_a_model() {
    let draft = LaunchDraft::new("native");
    assert_eq!(draft.to_body().unwrap_err(), DraftError::ModelRequired);
    assert!(!DraftError::ModelRequired.message().is_empty());
}

#[quench_test::test]
fn the_defaults_become_the_old_forms_body() {
    let mut draft = LaunchDraft::new("native");
    draft.model = " llama ".into();
    let body = draft.to_body().unwrap();
    assert_eq!(body.model, "llama");
    assert_eq!((body.host.as_str(), body.port), ("0.0.0.0", 8000));
    assert_eq!(body.namespace.as_deref(), Some("native"));
    assert_eq!(body.gpu_memory_utilization, Some(0.9));
    assert_eq!(body.quantization, None);
    assert_eq!(body.device, None);
    assert!(!body.enable_prefix_caching && !body.enable_tool_calling);
}

#[quench_test::test]
fn what_does_not_read_is_left_to_the_defaults() {
    let mut draft = LaunchDraft::new("native");
    draft.model = "m".into();
    draft.port = "abc".into();
    draft.max_model_len = "x".into();
    draft.gpu_util = "".into();
    draft.host = "  ".into();
    let body = draft.to_body().unwrap();
    assert_eq!((body.host.as_str(), body.port), ("0.0.0.0", 8000));
    assert_eq!(body.max_model_len, None);
    assert_eq!(body.gpu_memory_utilization, Some(0.9));
}

#[quench_test::test]
fn every_field_travels() {
    let mut draft = LaunchDraft::new("ml");
    draft.model = "m".into();
    draft.host = "127.0.0.1".into();
    draft.port = " 9000 ".into();
    draft.quantization = "awq".into();
    draft.dtype = "float16".into();
    draft.max_model_len = "8192".into();
    draft.limit_mm = r#"{"image": 4}"#.into();
    draft.gpu_util = "0.55".into();
    draft.prefix_caching = true;
    draft.tool_calling = true;
    draft.task = "embed".into();
    let body = draft.to_body().unwrap();
    assert_eq!((body.host.as_str(), body.port), ("127.0.0.1", 9000));
    assert_eq!(body.namespace.as_deref(), Some("ml"));
    assert_eq!(body.quantization.as_deref(), Some("awq"));
    assert_eq!(body.dtype.as_deref(), Some("float16"));
    assert_eq!(body.max_model_len, Some(8192));
    assert_eq!(body.limit_mm_per_prompt.as_deref(), Some(r#"{"image": 4}"#));
    assert_eq!(body.gpu_memory_utilization, Some(0.55));
    assert!(body.enable_prefix_caching && body.enable_tool_calling);
    assert_eq!(body.task.as_deref(), Some("embed"));
}

#[quench_test::test]
fn a_cpu_launch_has_no_memory_share() {
    let mut draft = LaunchDraft::new("native");
    draft.model = "m".into();
    draft.device = "cpu".into();
    assert!(draft.is_cpu());
    let body = draft.to_body().unwrap();
    assert_eq!(body.device.as_deref(), Some("cpu"));
    assert_eq!(body.gpu_memory_utilization, None);
}
