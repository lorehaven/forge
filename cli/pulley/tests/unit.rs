#[path = "unit/cli_tests.rs"]
mod cli_tests;
#[path = "unit/config_tests.rs"]
mod config_tests;
#[path = "unit/daemon_tests.rs"]
mod daemon_tests;
#[path = "unit/env_support.rs"]
mod env_support;
#[path = "unit/job_log_tests.rs"]
mod job_log_tests;
#[path = "unit/repl_tests.rs"]
mod repl_tests;
#[path = "unit/rsync_tests.rs"]
mod rsync_tests;
#[cfg(unix)]
#[path = "unit/service_unix_mod_tests.rs"]
mod service_unix_mod_tests;
#[cfg(unix)]
#[path = "unit/service_unix_runit_tests.rs"]
mod service_unix_runit_tests;
#[cfg(unix)]
#[path = "unit/service_unix_systemd_tests.rs"]
mod service_unix_systemd_tests;
