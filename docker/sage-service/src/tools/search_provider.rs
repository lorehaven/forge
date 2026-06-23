use async_trait::async_trait;

#[async_trait]
pub trait SearchProvider: Send + Sync {
    fn name(&self) -> &str;
    fn requires_api_key(&self) -> bool;
    async fn search(&self, query: &str) -> Result<String, String>;
}

pub struct SearchProviderRegistry {
    providers: std::collections::HashMap<String, Box<dyn SearchProvider>>,
    default_provider: String,
}

impl SearchProviderRegistry {
    pub fn new() -> Self {
        Self {
            providers: std::collections::HashMap::new(),
            default_provider: "duckduckgo".to_string(),
        }
    }

    pub fn register(&mut self, name: String, provider: Box<dyn SearchProvider>) {
        self.providers.insert(name, provider);
    }

    pub fn set_default(&mut self, name: String) {
        if self.providers.contains_key(&name) {
            self.default_provider = name;
        }
    }

    pub fn get(&self, name: Option<&str>) -> Option<&dyn SearchProvider> {
        let provider_name = name.unwrap_or(&self.default_provider);
        self.providers
            .get(provider_name)
            .map(|p| p.as_ref() as &dyn SearchProvider)
    }

    pub fn get_default(&self) -> &dyn SearchProvider {
        self.providers
            .get(&self.default_provider)
            .map(|p| p.as_ref() as &dyn SearchProvider)
            .expect("Default provider not registered")
    }

    pub fn list(&self) -> Vec<String> {
        self.providers.keys().cloned().collect()
    }
}

impl Default for SearchProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}
