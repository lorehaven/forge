use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolMetrics {
    pub calls: u64,
    pub successes: u64,
    pub failures: u64,
    pub timeouts: u64,
    pub total_duration_ms: u128,
    pub min_duration_ms: u128,
    pub max_duration_ms: u128,
}

impl ToolMetrics {
    pub fn new() -> Self {
        Self {
            calls: 0,
            successes: 0,
            failures: 0,
            timeouts: 0,
            total_duration_ms: 0,
            min_duration_ms: u128::MAX,
            max_duration_ms: 0,
        }
    }

    pub fn success_rate(&self) -> f64 {
        if self.calls == 0 {
            0.0
        } else {
            self.successes as f64 / self.calls as f64
        }
    }

    pub fn avg_duration_ms(&self) -> f64 {
        if self.calls == 0 {
            0.0
        } else {
            self.total_duration_ms as f64 / self.calls as f64
        }
    }

    pub fn record_execution(&mut self, duration_ms: u128, success: bool, timeout: bool) {
        self.calls += 1;
        if success {
            self.successes += 1;
        } else {
            self.failures += 1;
        }
        if timeout {
            self.timeouts += 1;
        }

        self.total_duration_ms += duration_ms;
        self.min_duration_ms = self.min_duration_ms.min(duration_ms);
        self.max_duration_ms = self.max_duration_ms.max(duration_ms);
    }
}

impl Default for ToolMetrics {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileMetrics {
    pub profile: String,
    pub total_calls: u64,
    pub total_successes: u64,
    pub total_failures: u64,
    pub tools: std::collections::HashMap<String, ToolMetrics>,
}

impl ProfileMetrics {
    pub fn new(profile: String) -> Self {
        Self {
            profile,
            total_calls: 0,
            total_successes: 0,
            total_failures: 0,
            tools: std::collections::HashMap::new(),
        }
    }

    pub fn success_rate(&self) -> f64 {
        if self.total_calls == 0 {
            0.0
        } else {
            self.total_successes as f64 / self.total_calls as f64
        }
    }

    pub fn update_from_tools(&mut self) {
        self.total_calls = 0;
        self.total_successes = 0;
        self.total_failures = 0;

        for metrics in self.tools.values() {
            self.total_calls += metrics.calls;
            self.total_successes += metrics.successes;
            self.total_failures += metrics.failures;
        }
    }
}

pub struct MetricsCollector {
    // Profile -> (Tool -> Metrics)
    data: Arc<DashMap<String, Arc<DashMap<String, ToolMetrics>>>>,
}

impl MetricsCollector {
    pub fn new() -> Self {
        Self {
            data: Arc::new(DashMap::new()),
        }
    }

    pub fn record_tool_execution(
        &self,
        profile: &str,
        tool_name: &str,
        duration_ms: u128,
        success: bool,
        timeout: bool,
    ) {
        let profile_map = self
            .data
            .entry(profile.to_string())
            .or_insert_with(|| Arc::new(DashMap::new()));

        profile_map
            .entry(tool_name.to_string())
            .or_default()
            .record_execution(duration_ms, success, timeout);
    }

    pub fn get_profile_metrics(&self, profile: &str) -> Option<ProfileMetrics> {
        self.data.get(profile).map(|profile_map| {
            let mut metrics = ProfileMetrics::new(profile.to_string());

            for entry in profile_map.iter() {
                metrics
                    .tools
                    .insert(entry.key().clone(), entry.value().clone());
            }

            metrics.update_from_tools();
            metrics
        })
    }

    pub fn get_all_profiles_metrics(&self) -> Vec<ProfileMetrics> {
        self.data
            .iter()
            .map(|entry| {
                let mut metrics = ProfileMetrics::new(entry.key().clone());

                for tool_entry in entry.value().iter() {
                    metrics
                        .tools
                        .insert(tool_entry.key().clone(), tool_entry.value().clone());
                }

                metrics.update_from_tools();
                metrics
            })
            .collect()
    }

    pub fn reset(&self) {
        self.data.clear();
    }

    pub fn reset_profile(&self, profile: &str) {
        self.data.remove(profile);
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}
