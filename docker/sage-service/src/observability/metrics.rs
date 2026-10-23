use quench_serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

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
    #[must_use]
    pub const fn new() -> Self {
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

    #[must_use]
    pub fn success_rate(&self) -> f64 {
        if self.calls == 0 {
            0.0
        } else {
            crate::numeric::u64_to_f64(self.successes) / crate::numeric::u64_to_f64(self.calls)
        }
    }

    #[must_use]
    pub fn avg_duration_ms(&self) -> f64 {
        if self.calls == 0 {
            0.0
        } else {
            crate::numeric::u128_to_f64(self.total_duration_ms)
                / crate::numeric::u64_to_f64(self.calls)
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
    #[must_use]
    pub fn new(profile: String) -> Self {
        Self {
            profile,
            total_calls: 0,
            total_successes: 0,
            total_failures: 0,
            tools: std::collections::HashMap::new(),
        }
    }

    #[must_use]
    pub fn success_rate(&self) -> f64 {
        if self.total_calls == 0 {
            0.0
        } else {
            crate::numeric::u64_to_f64(self.total_successes)
                / crate::numeric::u64_to_f64(self.total_calls)
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

#[derive(Debug)]
pub struct MetricsCollector {
    // Profile -> (Tool -> Metrics)
    data: Mutex<HashMap<String, HashMap<String, ToolMetrics>>>,
}

impl MetricsCollector {
    #[must_use]
    pub fn new() -> Self {
        Self {
            data: Mutex::new(HashMap::new()),
        }
    }

    fn data(&self) -> MutexGuard<'_, HashMap<String, HashMap<String, ToolMetrics>>> {
        self.data.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn record_tool_execution(
        &self,
        profile: &str,
        tool_name: &str,
        duration_ms: u128,
        success: bool,
        timeout: bool,
    ) {
        self.data()
            .entry(profile.to_string())
            .or_default()
            .entry(tool_name.to_string())
            .or_default()
            .record_execution(duration_ms, success, timeout);
    }

    #[must_use]
    pub fn get_profile_metrics(&self, profile: &str) -> Option<ProfileMetrics> {
        self.data().get(profile).map(|profile_map| {
            let mut metrics = ProfileMetrics::new(profile.to_string());

            for (name, tool) in profile_map {
                metrics.tools.insert(name.clone(), tool.clone());
            }

            metrics.update_from_tools();
            metrics
        })
    }

    #[must_use]
    pub fn get_all_profiles_metrics(&self) -> Vec<ProfileMetrics> {
        self.data()
            .iter()
            .map(|(profile, tools)| {
                let mut metrics = ProfileMetrics::new(profile.clone());

                for (name, tool) in tools {
                    metrics.tools.insert(name.clone(), tool.clone());
                }

                metrics.update_from_tools();
                metrics
            })
            .collect()
    }

    pub fn reset(&self) {
        self.data().clear();
    }

    pub fn reset_profile(&self, profile: &str) {
        self.data().remove(profile);
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}
