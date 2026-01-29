use crate::ui::interface::InteractionHandler;
use crate::plan::ExecutionPlan;
use crate::ui::render::{render_plan, render_model_progress, pretty_print_response, ModelLoadPhase};
use colored::Colorize;

#[derive(Debug)]
pub struct ReplMode;

impl InteractionHandler for ReplMode {
    fn render_plan(&self, plan: &ExecutionPlan) {
        render_plan(plan);
    }

    fn render_model_progress(&self, phase: ModelLoadPhase) {
        render_model_progress(phase);
    }

    fn print_message(&self, message: &str) {
        println!("{message}");
    }

    fn print_error(&self, error: &str) {
        eprintln!("{} {error}", "Error:".red().bold());
    }

    fn print_info(&self, info: &str) {
        println!("{}", info.dimmed());
    }

    fn print_response(&self, response: &str) {
        pretty_print_response(response);
    }

    fn print_debug(&self, message: &str) {
        eprintln!("{} {message}", "DEBUG:".yellow());
    }
}
