#[path = "integration/support.rs"]
mod support;

#[path = "integration/checkout_and_run_tests.rs"]
mod checkout_and_run_tests;
#[path = "integration/credentials_store_tests.rs"]
mod credentials_store_tests;
#[path = "integration/routers_api_projects_secrets_credentials_tests.rs"]
mod routers_api_projects_secrets_credentials_tests;
#[path = "integration/routers_api_repos_runs_tests.rs"]
mod routers_api_repos_runs_tests;
#[path = "integration/routers_ui_home_tests.rs"]
mod routers_ui_home_tests;
#[path = "integration/routers_ui_pipelines_http_tests.rs"]
mod routers_ui_pipelines_http_tests;
#[path = "integration/routers_ui_repos_tests.rs"]
mod routers_ui_repos_tests;
#[path = "integration/routers_ui_runs_http_tests.rs"]
mod routers_ui_runs_http_tests;
#[path = "integration/scan_tests.rs"]
mod scan_tests;
#[path = "integration/scheduler_projects_tests.rs"]
mod scheduler_projects_tests;
#[path = "integration/scheduler_queue_tests.rs"]
mod scheduler_queue_tests;
#[path = "integration/scheduler_worker_tests.rs"]
mod scheduler_worker_tests;
#[path = "integration/secrets_store_tests.rs"]
mod secrets_store_tests;
#[path = "integration/webhook_tests.rs"]
mod webhook_tests;
