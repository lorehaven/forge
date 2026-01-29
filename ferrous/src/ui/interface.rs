use crate::plan::ExecutionPlan;
pub use crate::ui::render::ModelLoadPhase;

pub trait InteractionHandler: Send + Sync {
    fn set_current_step(&self, _step_id: Option<usize>) {}
    fn render_plan(&self, plan: &ExecutionPlan);
    fn render_model_progress(&self, phase: ModelLoadPhase);
    fn print_message(&self, message: &str);
    fn print_error(&self, error: &str);
    fn print_info(&self, info: &str);

    // For tool output or intermediate responses
    fn print_response(&self, response: &str);

    // For streaming text from assistant
    fn print_stream_start(&self) {}
    fn print_stream_chunk(&self, _chunk: &str) {}
    fn print_stream_end(&self) {}

    fn print_stream_code_start(&self, _lang: &str) {}
    fn print_stream_code_chunk(&self, _chunk: &str) {}
    fn print_stream_code_end(&self) {}

    fn print_stream_tool_start(&self) {}
    fn print_stream_tool_chunk(&self, _chunk: &str) {}
    fn print_stream_tool_end(&self) {}

    // For debug info
    fn print_debug(&self, message: &str);
}
