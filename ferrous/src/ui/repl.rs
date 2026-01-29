use crate::plan::ExecutionPlan;
use crate::ui::interface::InteractionHandler;
use crate::ui::render::{
    ModelLoadPhase, pretty_print_response, render_model_progress, render_plan,
};
use colored::Colorize;
use std::io::Write;

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
        let mut stdout = std::io::stdout();
        let _ = write!(stdout, "{}", info.dimmed());
        let _ = stdout.flush();
    }

    fn print_response(&self, response: &str) {
        pretty_print_response(response);
    }

    fn print_stream_start(&self) {
        let mut stdout = std::io::stdout();
        let _ = write!(stdout, "{}", "│ ".dimmed());
        let _ = stdout.flush();
    }

    fn print_stream_chunk(&self, chunk: &str) {
        let mut stdout = std::io::stdout();
        let _ = write!(stdout, "{chunk}");
        let _ = stdout.flush();
    }

    fn print_stream_end(&self) {
        println!();
    }

    fn print_stream_code_start(&self, lang: &str) {
        let mut stdout = std::io::stdout();
        let _ = write!(
            stdout,
            "\n{} {}\n{} ",
            "┌──".dimmed(),
            lang.bright_yellow().bold(),
            "│".dimmed()
        );
        let _ = stdout.flush();
    }

    fn print_stream_code_chunk(&self, chunk: &str) {
        let mut stdout = std::io::stdout();
        for (i, segment) in chunk.split('\n').enumerate() {
            if i > 0 {
                let _ = write!(stdout, "\n{} ", "│".dimmed());
            }
            let _ = write!(stdout, "{}", segment.bright_white());
        }
        let _ = stdout.flush();
    }

    fn print_stream_code_end(&self) {
        let mut stdout = std::io::stdout();
        let _ = write!(stdout, "\r{}\n", "└──".dimmed());
        let _ = stdout.flush();
    }

    fn print_stream_tool_start(&self) {
        let mut stdout = std::io::stdout();
        let _ = write!(stdout, "\n  {} ", "🛠 Tool Call:".bright_yellow().bold());
        let _ = stdout.flush();
    }

    fn print_stream_tool_chunk(&self, chunk: &str) {
        let mut stdout = std::io::stdout();
        let _ = write!(stdout, "{}", chunk.bright_cyan());
        let _ = stdout.flush();
    }

    fn print_stream_tool_end(&self) {
        println!();
    }

    fn print_debug(&self, message: &str) {
        eprintln!("{} {message}", "DEBUG:".yellow());
    }
}
