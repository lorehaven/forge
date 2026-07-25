use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

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
    pub fn new(user_id: Option<String>, conversation_id: Option<String>, profile: String) -> Self {
        Self {
            timestamp: chrono::Utc::now().to_rfc3339(),
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

    pub fn estimate_cost(&self) -> f64 {
        // Simple cost estimation based on tokens and API calls
        // Adjust multipliers based on your actual pricing
        let token_cost = self.total_tokens_used as f64 * 0.000002; // $2 per 1M tokens (OpenAI GPT-4 pricing)
        let api_call_cost = self.total_api_calls as f64 * 0.001; // $0.001 per API call (example)
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
    pub fn from_requests(profile: String, requests: &[RequestCost]) -> Self {
        let total_duration_ms = requests.iter().map(|r| r.total_duration_ms).sum();
        let total_tokens_used = requests.iter().map(|r| r.total_tokens_used).sum();
        let total_api_calls = requests.iter().map(|r| r.total_api_calls).sum();

        let estimated_cost = requests.iter().map(|r| r.estimate_cost()).sum::<f64>();

        let request_count = requests.len() as u64;
        let avg_cost_per_request = if request_count > 0 {
            estimated_cost / request_count as f64
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
    pub fn from_requests(user_id: String, requests: &[RequestCost]) -> Self {
        let total_duration_ms = requests.iter().map(|r| r.total_duration_ms).sum();
        let total_tokens_used = requests.iter().map(|r| r.total_tokens_used).sum();
        let total_api_calls = requests.iter().map(|r| r.total_api_calls).sum();

        let estimated_cost = requests.iter().map(|r| r.estimate_cost()).sum::<f64>();

        let request_count = requests.len() as u64;
        let avg_cost_per_request = if request_count > 0 {
            estimated_cost / request_count as f64
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

pub struct CostTracker {
    // User ID -> Vec of RequestCosts
    user_costs: Arc<DashMap<String, Vec<RequestCost>>>,
    // Profile -> Vec of RequestCosts
    profile_costs: Arc<DashMap<String, Vec<RequestCost>>>,
}

impl CostTracker {
    pub fn new() -> Self {
        Self {
            user_costs: Arc::new(DashMap::new()),
            profile_costs: Arc::new(DashMap::new()),
        }
    }

    pub fn record_request_cost(&self, cost: RequestCost) {
        // Record by user
        if let Some(user_id) = &cost.user_id {
            self.user_costs
                .entry(user_id.clone())
                .or_default()
                .push(cost.clone());
        }

        // Record by profile
        self.profile_costs
            .entry(cost.profile.clone())
            .or_default()
            .push(cost);
    }

    pub fn get_user_costs(&self, user_id: &str) -> Option<UserCosts> {
        self.user_costs
            .get(user_id)
            .map(|requests| UserCosts::from_requests(user_id.to_string(), &requests))
    }

    pub fn get_profile_costs(&self, profile: &str) -> Option<ProfileCosts> {
        self.profile_costs
            .get(profile)
            .map(|requests| ProfileCosts::from_requests(profile.to_string(), &requests))
    }

    pub fn get_all_user_costs(&self) -> Vec<UserCosts> {
        self.user_costs
            .iter()
            .map(|entry| UserCosts::from_requests(entry.key().clone(), entry.value()))
            .collect()
    }

    pub fn get_all_profile_costs(&self) -> Vec<ProfileCosts> {
        self.profile_costs
            .iter()
            .map(|entry| ProfileCosts::from_requests(entry.key().clone(), entry.value()))
            .collect()
    }

    pub fn get_user_request_costs(&self, user_id: &str) -> Option<Vec<RequestCost>> {
        self.user_costs
            .get(user_id)
            .map(|requests| requests.clone())
    }

    pub fn reset(&self) {
        self.user_costs.clear();
        self.profile_costs.clear();
    }

    pub fn reset_user(&self, user_id: &str) {
        self.user_costs.remove(user_id);
    }

    pub fn reset_profile(&self, profile: &str) {
        self.profile_costs.remove(profile);
    }
}

impl Default for CostTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_cost_creation() {
        let cost = RequestCost::new(
            Some("user-1".to_string()),
            Some("conv-1".to_string()),
            "web_assistant".to_string(),
        );
        assert_eq!(cost.user_id, Some("user-1".to_string()));
        assert_eq!(cost.total_tokens_used, 0);
    }

    #[test]
    fn test_add_tool_cost() {
        let mut cost = RequestCost::new(
            Some("user-1".to_string()),
            Some("conv-1".to_string()),
            "web_assistant".to_string(),
        );

        cost.add_tool_cost(ToolCost {
            tool_name: "web_search".to_string(),
            tokens_used: 100,
            api_calls: 1,
            duration_ms: 500,
        });

        assert_eq!(cost.total_tokens_used, 100);
        assert_eq!(cost.total_api_calls, 1);
        assert_eq!(cost.total_duration_ms, 500);
    }

    #[test]
    fn test_cost_tracker() {
        let tracker = CostTracker::new();
        let mut cost = RequestCost::new(
            Some("user-1".to_string()),
            None,
            "web_assistant".to_string(),
        );
        cost.add_tool_cost(ToolCost {
            tool_name: "web_search".to_string(),
            tokens_used: 100,
            api_calls: 1,
            duration_ms: 500,
        });

        tracker.record_request_cost(cost);

        let user_costs = tracker.get_user_costs("user-1").unwrap();
        assert_eq!(user_costs.request_count, 1);
        assert_eq!(user_costs.total_tokens_used, 100);
    }

    #[test]
    fn test_estimate_cost() {
        let mut cost = RequestCost::new(None, None, "web_assistant".to_string());
        cost.total_tokens_used = 1_000_000; // 1M tokens
        cost.total_api_calls = 100;

        let estimated = cost.estimate_cost();
        // 1M tokens * $0.000002 + 100 calls * $0.001 = $2 + $0.1 = $2.1
        assert!((estimated - 2.1).abs() < 0.01);
    }
}
