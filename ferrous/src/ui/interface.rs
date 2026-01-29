use crate::plan::ExecutionPlan;
pub use crate::ui::render::ModelLoadPhase;

pub trait InteractionHandler: Send + Sync {
    fn render_plan(&self, plan: &ExecutionPlan);
    fn render_model_progress(&self, phase: ModelLoadPhase);
    fn print_message(&self, message: &str);
    fn print_error(&self, error: &str);
    fn print_info(&self, info: &str);
    
    // For tool output or intermediate responses
    fn print_response(&self, response: &str);
    
    // For debug info
    fn print_debug(&self, message: &str);
}
