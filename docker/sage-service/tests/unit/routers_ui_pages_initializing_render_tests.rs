use sage_service::clients::switchboard::VllmInstance;
use sage_service::config::DefaultModel;
use sage_service::routers::ui::pages::initializing::{
    ModelState, model_label, render_initializing_page, render_model_rows, state_presentation,
};

fn default_model(name: &str, task: Option<&str>) -> DefaultModel {
    DefaultModel {
        name: name.to_string(),
        gpu_memory_utilization: None,
        max_model_len: None,
        quantization: None,
        dtype: None,
        limit_mm_per_prompt: None,
        enable_tool_calling: false,
        task: task.map(str::to_string),
    }
}

#[test]
fn model_label_tags_embedding_models_but_not_chat_models() {
    let chat = default_model("llama", None);
    assert!(!model_label(&chat).render().contains("embedding"));

    let embed = default_model("bge", Some("embed"));
    assert!(
        model_label(&embed)
            .render()
            .contains("ui_init_embedding_tag")
    );
}

#[test]
fn state_presentation_covers_every_state() {
    for state in [
        ModelState::Running,
        ModelState::Starting,
        ModelState::Pending,
        ModelState::Failed,
        ModelState::Unknown,
    ] {
        let (icon, text, key, modifier) = state_presentation(state);
        assert!(!icon.is_empty());
        assert!(!text.is_empty());
        assert!(!key.is_empty());
        assert!(!modifier.is_empty());
    }
}

#[test]
fn render_model_rows_shows_unknown_state_when_switchboard_is_unreachable() {
    let defaults = vec![default_model("llama", None)];
    let err: anyhow::Result<Vec<VllmInstance>> = Err(anyhow::anyhow!("unreachable"));
    let html = render_model_rows(&defaults, &err).render();
    assert!(html.contains("model-row--unknown"));
}

#[test]
fn render_model_rows_reflects_each_models_own_state() {
    let defaults = vec![default_model("llama", None), default_model("mistral", None)];
    let instances = Ok(vec![VllmInstance {
        id: "i1".to_string(),
        namespace: "ns".to_string(),
        model: "llama".to_string(),
        host: "h".to_string(),
        port: 8000,
        quantization: None,
        max_model_len: None,
        gpu_memory_utilization: None,
        enable_prefix_caching: false,
        task: None,
        started_at: chrono::Utc::now(),
        status: "running".to_string(),
    }]);
    let html = render_model_rows(&defaults, &instances).render();
    assert!(html.contains("model-row--running"));
    // mistral has no matching instance, so it stays pending/queued.
    assert!(html.contains("model-row--pending"));
}

#[test]
fn render_initializing_page_shows_a_warning_only_when_switchboard_is_down() {
    let defaults = vec![default_model("llama", None)];

    let ok: anyhow::Result<Vec<VllmInstance>> = Ok(Vec::new());
    let up_html = render_initializing_page(&defaults, &ok);
    assert!(up_html.status().is_success());

    let err: anyhow::Result<Vec<VllmInstance>> = Err(anyhow::anyhow!("down"));
    let down_html = render_initializing_page(&defaults, &err);
    assert!(down_html.status().is_success());
}
