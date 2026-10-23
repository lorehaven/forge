pub mod state;
pub mod validate;

pub use state::AppState;
pub use validate::report_toolchain;

pub fn init_tracing() {
    quench_starter::logging::init();
    quench_log::info!("Conveyor service starting");
}
