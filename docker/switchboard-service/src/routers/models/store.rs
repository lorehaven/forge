use super::discovery::{fetch_gguf_models, fetch_hf_models};
use super::{Model, VllmArchitecturesFile};
use quench_db::Db;
use quench_db::prelude::{Crud, Model as OrmModel, Repository};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use tokio::sync::OnceCell;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct VllmArchitecture {
    pub id: String,
}

impl OrmModel for VllmArchitecture {
    fn table_name() -> String {
        "switchboard.vllm_architectures".to_string()
    }

    fn columns() -> Vec<&'static str> {
        vec!["id"]
    }

    fn primary_key_name() -> String {
        "id".to_string()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct CachedModel {
    pub path: String,
    pub data: serde_json::Value,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl OrmModel for CachedModel {
    fn table_name() -> String {
        "switchboard.model_cache".to_string()
    }

    fn columns() -> Vec<&'static str> {
        vec!["path", "data", "updated_at"]
    }

    fn primary_key_name() -> String {
        "path".to_string()
    }
}

pub struct ModelStore {
    pub db: Db,
    model_repo: Repository<CachedModel>,
    arch_repo: Repository<VllmArchitecture>,
    // Keep in-memory cache for performance
    cache: RwLock<HashMap<String, Model>>,
    architectures: RwLock<HashSet<String>>,
}

impl ModelStore {
    #[tracing::instrument(skip(db))]
    pub async fn init(db: Db) -> Self {
        let arch_file = Self::load_architectures_file();

        let store = Self {
            db: db.clone(),
            model_repo: db.repository::<CachedModel>(),
            arch_repo: db.repository::<VllmArchitecture>(),
            cache: RwLock::new(HashMap::new()),
            architectures: RwLock::new(arch_file),
        };

        // Sync architectures: load from DB first, then merge with file
        if let Ok(existing_archs) = store.arch_repo.list().await {
            let mut archs = store.architectures.write().unwrap();
            for arch in existing_archs {
                archs.insert(arch.id);
            }
        }

        // Sync architectures to DB (the ones from file that might be new)
        let ids: Vec<String> = store
            .architectures
            .read()
            .unwrap()
            .iter()
            .cloned()
            .collect();
        for id in ids {
            store.arch_repo.create(&VllmArchitecture { id }).await.ok();
        }

        store
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_all_models(&self) -> Vec<Model> {
        match self.model_repo.list().await {
            Ok(cached_models) => cached_models
                .into_iter()
                .filter_map(|m| match serde_json::from_value::<Model>(m.data) {
                    Ok(model) => Some(model),
                    Err(e) => {
                        tracing::error!("Failed to deserialize model at {}: {}", m.path, e);
                        None
                    }
                })
                .collect(),
            Err(e) => {
                tracing::error!("Failed to list models from DB: {}", e);
                self.cache.read().unwrap().values().cloned().collect()
            }
        }
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_all_paths(&self) -> Vec<String> {
        match self.model_repo.list().await {
            Ok(cached_models) => cached_models.into_iter().map(|m| m.path).collect(),
            Err(e) => {
                tracing::error!("Failed to list model paths from DB: {}", e);
                self.cache.read().unwrap().keys().cloned().collect()
            }
        }
    }

    pub fn clear_in_memory_cache(&self) {
        self.cache.write().unwrap().clear();
    }

    fn load_architectures_file() -> HashSet<String> {
        let path = std::env::var("VLLM_ARCHITECTURES_FILE")
            .unwrap_or_else(|_| "/opt/vllm_architectures.json".to_string());

        if let Ok(contents) = std::fs::read_to_string(&path)
            && let Ok(parsed) = serde_json::from_str::<VllmArchitecturesFile>(&contents)
        {
            let archs: HashSet<String> = parsed.architectures.into_iter().collect();
            tracing::info!(
                "Registered {} vLLM architectures from file: {}",
                archs.len(),
                path
            );
            return archs;
        }

        tracing::warn!(
            "vLLM architecture file not found or invalid: {}. Falling back to default list.",
            path
        );

        // Fallback common architectures if file is missing
        [
            "LlamaForCausalLM",
            "MistralForCausalLM",
            "MixtralForCausalLM",
            "Qwen2ForCausalLM",
            "GPT2LMHeadModel",
            "GPTNeoXForCausalLM",
            "GPTBigCodeForCausalLM",
            "OPTForCausalLM",
            "BloomForCausalLM",
            "FalconForCausalLM",
            "MPTForCausalLM",
            "PhiForCausalLM",
            "GemmaForCausalLM",
            "DbrxForCausalLM",
            "ArcticForCausalLM",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    }

    pub fn is_vllm_supported(&self, architecture: &str) -> bool {
        self.architectures.read().unwrap().contains(architecture)
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_model(&self, path: &str) -> Option<Model> {
        // Try in-memory cache first
        if let Some(cached) = self.cache.read().unwrap().get(path).cloned() {
            return Some(cached);
        }

        // Try DB
        if let Ok(Some(cached_db)) = self.model_repo.read(path).await
            && let Ok(model) = serde_json::from_value::<Model>(cached_db.data)
        {
            self.cache
                .write()
                .unwrap()
                .insert(path.to_string(), model.clone());
            return Some(model);
        }

        None
    }

    #[tracing::instrument(skip(self, model), fields(path = %model.path))]
    pub async fn insert_model(&self, model: &Model) {
        self.cache
            .write()
            .unwrap()
            .insert(model.path.clone(), model.clone());

        let cached_model = CachedModel {
            path: model.path.clone(),
            data: serde_json::to_value(model).unwrap(),
            updated_at: chrono::Utc::now(),
        };

        if let Ok(Some(_)) = self.model_repo.read(&model.path).await {
            if let Err(e) = self.model_repo.update(&cached_model).await {
                tracing::error!("Failed to update model {}: {}", model.path, e);
            }
        } else {
            if let Err(e) = self.model_repo.create(&cached_model).await {
                tracing::error!("Failed to create model {}: {}", model.path, e);
            }
        }
    }

    #[tracing::instrument(skip(self))]
    pub async fn remove_model(&self, path: &str) {
        self.cache.write().unwrap().remove(path);
        if let Err(e) = self.model_repo.delete(path).await {
            tracing::error!("Failed to remove model {} from DB: {}", path, e);
        }
    }
}

pub static MODEL_STORE: OnceCell<Arc<ModelStore>> = OnceCell::const_new();

pub async fn init_model_store(db: Db) {
    let store = Arc::new(ModelStore::init(db).await);
    if MODEL_STORE.set(store).is_err() {
        tracing::warn!("Model store already initialized");
    }
}

pub fn get_store() -> Arc<ModelStore> {
    MODEL_STORE
        .get()
        .expect("Model store not initialized")
        .clone()
}

#[tracing::instrument]
pub async fn warm_model_cache() {
    fetch_hf_models().await;
    fetch_gguf_models().await;
}
