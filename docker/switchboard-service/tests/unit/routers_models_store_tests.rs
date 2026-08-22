//! `ModelStore` runs entirely through `quench_db`'s `Crud`/`Repository`
//! abstraction (raw SQL only in the raw-SQL services like workbench), so it
//! works unmodified against `Db::connect("")`'s in-memory backend - no real
//! Postgres needed here.

use quench_db::prelude::Db;
use switchboard_service::routers::models::store::ModelStore;
use switchboard_service::routers::models::types::{Context, Model, Quant};

fn sample_model(path: &str) -> Model {
    Model {
        source: "HF".to_string(),
        name: format!("model at {path}"),
        path: path.to_string(),
        architecture: Some("LlamaForCausalLM".to_string()),
        vllm_supported: true,
        quant: Quant::FP16,
        context: Context::Size4096,
        layers: 32,
        hidden_size: 4096,
        params_billion: 7.0,
        estimates: vec![],
    }
}

async fn store() -> ModelStore {
    let db = Db::connect("").await.expect("in-memory database");
    ModelStore::init(db).await
}

#[tokio::test]
async fn insert_then_get_model_round_trips_through_the_db() {
    let store = store().await;
    let model = sample_model("/mnt/dev/huggingface/hub/llama");

    store.insert_model(&model).await;
    let fetched = store.get_model(&model.path).await.expect("model present");

    assert_eq!(fetched.name, model.name);
    assert_eq!(fetched.architecture, model.architecture);
}

#[tokio::test]
async fn insert_model_twice_updates_rather_than_duplicating() {
    let store = store().await;
    let mut model = sample_model("/mnt/dev/huggingface/hub/llama-updated");
    store.insert_model(&model).await;

    model.params_billion = 13.0;
    store.insert_model(&model).await;

    let all = store.get_all_models().await;
    let matching: Vec<_> = all.iter().filter(|m| m.path == model.path).collect();
    assert_eq!(matching.len(), 1);
    assert_eq!(matching[0].params_billion, 13.0);
}

#[tokio::test]
async fn get_model_for_an_unknown_path_is_none() {
    let store = store().await;
    assert!(store.get_model("/does/not/exist").await.is_none());
}

#[tokio::test]
async fn remove_model_deletes_it_from_the_db_and_cache() {
    let store = store().await;
    let model = sample_model("/mnt/dev/huggingface/hub/to-remove");
    store.insert_model(&model).await;
    assert!(store.get_model(&model.path).await.is_some());

    store.remove_model(&model.path).await;
    assert!(store.get_model(&model.path).await.is_none());
}

#[tokio::test]
async fn get_all_paths_lists_every_inserted_model() {
    let store = store().await;
    store
        .insert_model(&sample_model("/mnt/dev/huggingface/hub/a"))
        .await;
    store
        .insert_model(&sample_model("/mnt/dev/huggingface/hub/b"))
        .await;

    let mut paths = store.get_all_paths().await;
    paths.sort();
    assert_eq!(
        paths,
        vec![
            "/mnt/dev/huggingface/hub/a".to_string(),
            "/mnt/dev/huggingface/hub/b".to_string(),
        ]
    );
}

#[tokio::test]
async fn clear_in_memory_cache_does_not_lose_db_backed_reads() {
    let store = store().await;
    let model = sample_model("/mnt/dev/huggingface/hub/cached");
    store.insert_model(&model).await;

    store.clear_in_memory_cache();

    // The in-memory cache is just a read-through optimization; clearing it
    // must not lose data the DB still has.
    assert!(store.get_model(&model.path).await.is_some());
}

#[tokio::test]
async fn is_vllm_supported_reflects_the_loaded_architecture_list() {
    let store = store().await;
    // `load_architectures_file` falls back to a fixed default list when
    // `VLLM_ARCHITECTURES_FILE` is unset/invalid, which always includes this.
    assert!(store.is_vllm_supported("LlamaForCausalLM"));
    assert!(!store.is_vllm_supported("NotARealArchitectureForCausalLM"));
}

#[tokio::test]
async fn get_all_models_is_empty_for_a_freshly_initialized_store() {
    let store = store().await;
    assert!(store.get_all_models().await.is_empty());
}
