use quench_serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCost {
    pub tool_name: String,
    pub tokens_used: u64,
    pub api_calls: u32,
    pub duration_ms: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestCost {
    pub timestamp: String,
    pub user_id: Option<String>,
    pub conversation_id: Option<String>,
    pub profile: String,
    pub tools: Vec<ToolCost>,
    pub total_duration_ms: u128,
    pub total_tokens_used: u64,
    pub total_api_calls: u32,
}

impl RequestCost {
    #[must_use]
    pub fn new(user_id: Option<String>, conversation_id: Option<String>, profile: String) -> Self {
        Self {
            timestamp: quench_core::time::Time::now().to_rfc3339(),
            user_id,
            conversation_id,
            profile,
            tools: Vec::new(),
            total_duration_ms: 0,
            total_tokens_used: 0,
            total_api_calls: 0,
        }
    }

    pub fn add_tool_cost(&mut self, cost: ToolCost) {
        self.total_tokens_used += cost.tokens_used;
        self.total_api_calls += cost.api_calls;
        self.total_duration_ms += cost.duration_ms;
        self.tools.push(cost);
    }

    #[must_use]
    pub fn estimate_cost(&self) -> f64 {
        // Simple cost estimate; adjust multipliers below to your actual pricing.
        let token_cost = crate::numeric::u64_to_f64(self.total_tokens_used) * 0.000_002; // $2 per 1M tokens (OpenAI GPT-4 pricing)
        let api_call_cost = f64::from(self.total_api_calls) * 0.001; // $0.001 per API call (example)
        token_cost + api_call_cost
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileCosts {
    pub profile: String,
    pub request_count: u64,
    pub total_duration_ms: u128,
    pub total_tokens_used: u64,
    pub total_api_calls: u32,
    pub estimated_cost: f64,
    pub avg_cost_per_request: f64,
}

impl ProfileCosts {
    #[must_use]
    pub fn from_requests(profile: String, requests: &[RequestCost]) -> Self {
        let total_duration_ms = requests.iter().map(|r| r.total_duration_ms).sum();
        let total_tokens_used = requests.iter().map(|r| r.total_tokens_used).sum();
        let total_api_calls = requests.iter().map(|r| r.total_api_calls).sum();

        let estimated_cost = requests.iter().map(RequestCost::estimate_cost).sum::<f64>();

        let request_count = u64::try_from(requests.len()).unwrap_or(u64::MAX);
        let avg_cost_per_request = if request_count > 0 {
            estimated_cost / crate::numeric::u64_to_f64(request_count)
        } else {
            0.0
        };

        Self {
            profile,
            request_count,
            total_duration_ms,
            total_tokens_used,
            total_api_calls,
            estimated_cost,
            avg_cost_per_request,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserCosts {
    pub user_id: String,
    pub request_count: u64,
    pub total_duration_ms: u128,
    pub total_tokens_used: u64,
    pub total_api_calls: u32,
    pub estimated_cost: f64,
    pub avg_cost_per_request: f64,
}

impl UserCosts {
    #[must_use]
    pub fn from_requests(user_id: String, requests: &[RequestCost]) -> Self {
        let total_duration_ms = requests.iter().map(|r| r.total_duration_ms).sum();
        let total_tokens_used = requests.iter().map(|r| r.total_tokens_used).sum();
        let total_api_calls = requests.iter().map(|r| r.total_api_calls).sum();

        let estimated_cost = requests.iter().map(RequestCost::estimate_cost).sum::<f64>();

        let request_count = u64::try_from(requests.len()).unwrap_or(u64::MAX);
        let avg_cost_per_request = if request_count > 0 {
            estimated_cost / crate::numeric::u64_to_f64(request_count)
        } else {
            0.0
        };

        Self {
            user_id,
            request_count,
            total_duration_ms,
            total_tokens_used,
            total_api_calls,
            estimated_cost,
            avg_cost_per_request,
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[derive(Debug)]
pub struct CostTracker {
    // User ID -> Vec of RequestCosts
    user_costs: Mutex<HashMap<String, Vec<RequestCost>>>,
    // Profile -> Vec of RequestCosts
    profile_costs: Mutex<HashMap<String, Vec<RequestCost>>>,
}

impl CostTracker {
    #[must_use]
    pub fn new() -> Self {
        Self {
            user_costs: Mutex::new(HashMap::new()),
            profile_costs: Mutex::new(HashMap::new()),
        }
    }

    pub fn record_request_cost(&self, cost: RequestCost) {
        // Record by user
        if let Some(user_id) = &cost.user_id {
            lock(&self.user_costs)
                .entry(user_id.clone())
                .or_default()
                .push(cost.clone());
        }

        // Record by profile
        lock(&self.profile_costs)
            .entry(cost.profile.clone())
            .or_default()
            .push(cost);
    }

    #[must_use]
    pub fn get_user_costs(&self, user_id: &str) -> Option<UserCosts> {
        lock(&self.user_costs)
            .get(user_id)
            .map(|requests| UserCosts::from_requests(user_id.to_string(), requests))
    }

    #[must_use]
    pub fn get_profile_costs(&self, profile: &str) -> Option<ProfileCosts> {
        lock(&self.profile_costs)
            .get(profile)
            .map(|requests| ProfileCosts::from_requests(profile.to_string(), requests))
    }

    #[must_use]
    pub fn get_all_user_costs(&self) -> Vec<UserCosts> {
        lock(&self.user_costs)
            .iter()
            .map(|(user_id, requests)| UserCosts::from_requests(user_id.clone(), requests))
            .collect()
    }

    #[must_use]
    pub fn get_all_profile_costs(&self) -> Vec<ProfileCosts> {
        lock(&self.profile_costs)
            .iter()
            .map(|(profile, requests)| ProfileCosts::from_requests(profile.clone(), requests))
            .collect()
    }

    #[must_use]
    pub fn get_user_request_costs(&self, user_id: &str) -> Option<Vec<RequestCost>> {
        lock(&self.user_costs).get(user_id).cloned()
    }

    pub fn reset(&self) {
        lock(&self.user_costs).clear();
        lock(&self.profile_costs).clear();
    }

    pub fn reset_user(&self, user_id: &str) {
        lock(&self.user_costs).remove(user_id);
    }

    pub fn reset_profile(&self, profile: &str) {
        lock(&self.profile_costs).remove(profile);
    }
}

impl Default for CostTracker {
    fn default() -> Self {
        Self::new()
    }
}
