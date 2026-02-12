use crate::config::SamplingConfig;
use crate::core::Agent;
use colored::Colorize;
use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum StepStatus {
    Pending,
    Running,
    Done,
    Failed(String),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlanStep {
    pub id: usize,
    pub description: String,
    pub status: StepStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionPlan {
    pub steps: Vec<PlanStep>,
}

impl ExecutionPlan {
    #[must_use]
    pub fn new(descriptions: Vec<String>) -> Self {
        let steps = descriptions
            .into_iter()
            .enumerate()
            .map(|(i, desc)| PlanStep {
                id: i + 1,
                description: desc,
                status: StepStatus::Pending,
            })
            .collect();

        Self { steps }
    }

    pub fn mark_running(&mut self, id: usize) {
        if let Some(s) = self.steps.iter_mut().find(|s| s.id == id) {
            s.status = StepStatus::Running;
        }
    }

    pub fn mark_done(&mut self, id: usize) {
        if let Some(s) = self.steps.iter_mut().find(|s| s.id == id) {
            s.status = StepStatus::Done;
        }
    }

    pub fn mark_failed(&mut self, id: usize, reason: String) {
        if let Some(s) = self.steps.iter_mut().find(|s| s.id == id) {
            s.status = StepStatus::Failed(reason);
        }
    }
}

impl fmt::Display for ExecutionPlan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for step in &self.steps {
            let symbol = match &step.status {
                StepStatus::Pending => "[ ]".dimmed(),
                StepStatus::Running => "->".bright_cyan().bold(),
                StepStatus::Done => "[v]".bright_green().bold(),
                StepStatus::Failed(_) => "[x]".bright_red().bold(),
            };

            let desc = match &step.status {
                StepStatus::Running => step.description.bright_white().bold(),
                StepStatus::Done => step.description.dimmed(),
                StepStatus::Failed(_) => step.description.bright_red(),
                StepStatus::Pending => step.description.normal(),
            };

            writeln!(f, "{} {}. {}", symbol, step.id, desc)?;
        }
        Ok(())
    }
}

pub async fn execute_plan(
    agent: &mut Agent,
    mut plan: ExecutionPlan,
    sampling: SamplingConfig,
    is_debug: bool,
    interaction: &dyn crate::ui::interface::InteractionHandler,
) -> anyhow::Result<()> {
    for step in plan.steps.clone() {
        interaction.set_current_step(Some(step.id));
        plan.mark_running(step.id);
        interaction.render_plan(&plan);

        // Count messages before execution to detect if tool was called
        let messages_before = agent.messages.len();

        // Prepend execution instruction to ensure model uses tools
        let execution_prompt = format!(
            "EXECUTE THIS STEP NOW by calling the required tools. You MUST make tool calls to complete this step. DO NOT just say what you will do or mark it as done without action. Step: {}",
            step.description
        );

        let result = agent
            .stream(&execution_prompt, sampling.clone(), is_debug, interaction)
            .await;

        match result {
            Ok(resp) => {
                // Check if any tool was actually called
                let messages_after = agent.messages.len();
                let tool_called = messages_after > messages_before + 1; // assistant msg + at least one tool result

                if is_debug {
                    interaction.print_debug(&format!(
                        "Messages before: {}, after: {}, tool_called: {}",
                        messages_before, messages_after, tool_called
                    ));
                }

                if !tool_called && requires_tool_call(&step.description) {
                    interaction.print_error(&format!(
                        "Step '{}' requires a tool call but none was made. Response was: '{}'",
                        step.description,
                        resp.chars().take(100).collect::<String>()
                    ));
                    plan.mark_failed(step.id, "No tool call made when required".to_string());
                    interaction.render_plan(&plan);
                    return Err(anyhow::anyhow!("Execution failed: step required tool call but agent only responded with text"));
                }

                if is_debug && is_explanatory_step(&step.description) {
                    interaction.print_debug("\nResponse:");
                    interaction.print_response(&resp);
                }
                plan.mark_done(step.id);
                interaction.render_plan(&plan);
            }
            Err(e) => {
                plan.mark_failed(step.id, e.to_string());
                interaction.render_plan(&plan);
                interaction.print_error(&e.to_string());
                return Err(e);
            }
        }

        interaction.render_plan(&plan);
    }

    interaction.set_current_step(None);
    Ok(())
}

/// Check if a step description indicates a tool call is required
fn requires_tool_call(step: &str) -> bool {
    let step_lower = step.to_lowercase();

    // Skip steps that are purely informational
    if step_lower.starts_with("answer") {
        return false;
    }

    // Most action verbs require tool calls
    step_lower.contains("check") ||
    step_lower.contains("review") ||
    step_lower.contains("lint") ||
    step_lower.contains("analyze") ||
    step_lower.contains("search") ||
    step_lower.contains("find") ||
    step_lower.contains("list") ||
    step_lower.contains("read") ||
    step_lower.contains("write") ||
    step_lower.contains("create") ||
    step_lower.contains("modify") ||
    step_lower.contains("replace") ||
    step_lower.contains("delete") ||
    step_lower.contains("run") ||
    step_lower.contains("execute") ||
    step_lower.contains("suggest") ||
    step_lower.contains("show") ||
    step_lower.contains("display") ||
    step_lower.contains("get")
}

fn is_explanatory_step(step: &str) -> bool {
    let s = step.to_lowercase();
    s.starts_with("modify")
        || s.starts_with("replace")
        || s.starts_with("fix")
        || s.starts_with("add")
        || s.starts_with("remove")
        || s.starts_with("write")
        || s.starts_with("create")
        || s.starts_with("delete")
        || s.starts_with("move")
        || s.starts_with("rename")
}
