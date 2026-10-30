// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports)]

#[path = "unit/client_tests.rs"]
mod client_tests;
#[path = "unit/clients_switchboard_tests.rs"]
mod clients_switchboard_tests;
#[path = "unit/clients_vllm_tests.rs"]
mod clients_vllm_tests;
#[path = "unit/config_settings_tests.rs"]
mod config_settings_tests;
#[path = "unit/domain_context_tests.rs"]
mod domain_context_tests;
#[path = "unit/domain_conversation_tests.rs"]
mod domain_conversation_tests;
#[path = "unit/domain_models_tests.rs"]
mod domain_models_tests;
#[path = "unit/env_support.rs"]
mod env_support;
#[path = "unit/files_chunker_tests.rs"]
mod files_chunker_tests;
#[path = "unit/files_embedder_tests.rs"]
mod files_embedder_tests;
#[path = "unit/files_extractor_tests.rs"]
mod files_extractor_tests;
#[path = "unit/files_images_tests.rs"]
mod files_images_tests;
#[path = "unit/files_rag_tests.rs"]
mod files_rag_tests;
#[path = "unit/grounding_pipeline_tests.rs"]
mod grounding_pipeline_tests;
#[path = "unit/grounding_tests.rs"]
mod grounding_tests;
#[path = "unit/mock_estate.rs"]
mod mock_estate;
#[path = "unit/observability_audit_tests.rs"]
mod observability_audit_tests;
#[path = "unit/observability_cost_tracking_tests.rs"]
mod observability_cost_tracking_tests;
#[path = "unit/observability_metrics_tests.rs"]
mod observability_metrics_tests;
#[path = "unit/routers_chat_tests.rs"]
mod routers_chat_tests;
#[path = "unit/routers_conversations_tests.rs"]
mod routers_conversations_tests;
#[path = "unit/routers_files_api_tests.rs"]
mod routers_files_api_tests;
#[path = "unit/routers_files_tests.rs"]
mod routers_files_tests;
#[path = "unit/routers_models_tests.rs"]
mod routers_models_tests;
#[path = "unit/routers_turns_events_tests.rs"]
mod routers_turns_events_tests;
#[path = "unit/routers_turns_tests.rs"]
mod routers_turns_tests;
#[path = "unit/routers_ui_context_builder_tests.rs"]
mod routers_ui_context_builder_tests;
#[path = "unit/routers_ui_pages_auth_tests.rs"]
mod routers_ui_pages_auth_tests;
#[path = "unit/routers_wiring_tests.rs"]
mod routers_wiring_tests;
#[path = "unit/runtime_rate_limiter_tests.rs"]
mod runtime_rate_limiter_tests;
#[path = "unit/runtime_retry_policy_tests.rs"]
mod runtime_retry_policy_tests;
#[path = "unit/startup_default_models_tests.rs"]
mod startup_default_models_tests;
#[path = "unit/startup_mod_tests.rs"]
mod startup_mod_tests;
#[path = "unit/startup_validate_tests.rs"]
mod startup_validate_tests;
#[path = "unit/tools_calculator_tests.rs"]
mod tools_calculator_tests;
#[path = "unit/tools_capabilities_tests.rs"]
mod tools_capabilities_tests;
#[path = "unit/tools_code_executor_tests.rs"]
mod tools_code_executor_tests;
#[path = "unit/tools_command_tests.rs"]
mod tools_command_tests;
#[path = "unit/tools_file_ops_tests.rs"]
mod tools_file_ops_tests;
#[path = "unit/tools_mod_tests.rs"]
mod tools_mod_tests;
#[path = "unit/tools_parser_tests.rs"]
mod tools_parser_tests;
#[path = "unit/tools_search_providers_tests.rs"]
mod tools_search_providers_tests;
#[path = "unit/tools_tests.rs"]
mod tools_tests;
#[path = "unit/tools_web_search_tests.rs"]
mod tools_web_search_tests;

fn main() {
    // Tests that only need a `SwitchboardClient` to construct (and never expect it to answer) rely on
    // these. They are set here, before any test thread exists, so that no test has to write them:
    // the tests that mock switchboard set other values under `env_lock`, and an unlocked write from
    // a test running beside them would replace theirs. Loopback ports nothing listens on refuse a
    // connection at once, where a hostname can take most of a minute to fail on a resolver timeout.
    quench_core::env::set("GATEHOUSE_URL", "http://127.0.0.1:1");
    quench_core::env::set("SWITCHBOARD_URL", "http://127.0.0.1:1");
    quench_core::env::set("CLIENT_SECRET_SAGE_SWITCHBOARD", "test-secret");
    std::process::exit(quench_test::run_registered());
}
