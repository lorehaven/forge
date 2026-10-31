// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports)]

#[path = "unit/artifacts_tests.rs"]
mod artifacts_tests;
#[path = "unit/client_tests.rs"]
mod client_tests;
#[path = "unit/config_settings_tests.rs"]
mod config_settings_tests;
#[path = "unit/domain_run_tests.rs"]
mod domain_run_tests;
#[path = "unit/domain_status_tests.rs"]
mod domain_status_tests;
#[path = "unit/env_support.rs"]
mod env_support;
#[path = "unit/executors_export_tests.rs"]
mod executors_export_tests;
#[path = "unit/executors_kubernetes_tests.rs"]
mod executors_kubernetes_tests;
#[path = "unit/executors_manifest_tests.rs"]
mod executors_manifest_tests;
#[path = "unit/executors_mock_tests.rs"]
mod executors_mock_tests;
#[path = "unit/executors_native_tests.rs"]
mod executors_native_tests;
#[path = "unit/launch_tests.rs"]
mod launch_tests;
#[path = "unit/notifications_tests.rs"]
mod notifications_tests;
#[path = "unit/providers_generic_tests.rs"]
mod providers_generic_tests;
#[path = "unit/providers_github_tests.rs"]
mod providers_github_tests;
#[path = "unit/providers_mock_tests.rs"]
mod providers_mock_tests;
#[path = "unit/routers_api_authz_tests.rs"]
mod routers_api_authz_tests;
#[path = "unit/routers_api_overview_tests.rs"]
mod routers_api_overview_tests;
#[path = "unit/routers_api_routing_tests.rs"]
mod routers_api_routing_tests;
#[path = "unit/routers_api_stream_tests.rs"]
mod routers_api_stream_tests;
#[path = "unit/scan_tests.rs"]
mod scan_tests;
#[path = "unit/scheduler_privilege_tests.rs"]
mod scheduler_privilege_tests;
#[path = "unit/secrets_crypto_tests.rs"]
mod secrets_crypto_tests;
#[path = "unit/secrets_redact_tests.rs"]
mod secrets_redact_tests;
#[path = "unit/startup_validate_tests.rs"]
mod startup_validate_tests;
#[path = "unit/workspace_checkout_tests.rs"]
mod workspace_checkout_tests;

quench_test::main!();
