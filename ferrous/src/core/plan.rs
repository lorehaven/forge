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
    const MAX_NO_TOOL_RETRIES: usize = 2;

    for step in plan.steps.clone() {
        interaction.set_current_step(Some(step.id));
        plan.mark_running(step.id);
        interaction.render_plan(&plan);

        // Count messages before execution to detect if tool was called
        let messages_before = agent.messages.len();

        let mut attempt = 0usize;
        loop {
            // Prepend execution instruction to ensure model uses tools
            let execution_prompt = if attempt == 0 {
                format!(
                    "EXECUTE THIS STEP NOW by calling the required tools. You MUST make tool calls to complete this step. DO NOT just say what you will do or mark it as done without action. Step: {}",
                    step.description
                )
            } else {
                format!(
                    "RETRY STEP {}. Previous response did not include a tool call. You MUST call at least one tool now. Do not explain, do not summarize. Step: {}",
                    step.id, step.description
                )
            };

            let result = agent
                .stream(&execution_prompt, sampling.clone(), is_debug, interaction)
                .await;

            match result {
                Ok(outcome) => {
                    let tool_called = outcome.tool_calls_executed > 0;

                    if is_debug {
                        interaction.print_debug(&format!(
                            "Messages before: {}, attempt: {}, tool_calls_executed: {}, tool_called: {}",
                            messages_before,
                            attempt + 1,
                            outcome.tool_calls_executed,
                            tool_called
                        ));
                    }

                    if !tool_called && requires_tool_call(&step.description) {
                        if attempt < MAX_NO_TOOL_RETRIES {
                            if is_debug {
                                interaction.print_debug(&format!(
                                    "No tool call made for step {}. Retrying ({}/{})...",
                                    step.id,
                                    attempt + 1,
                                    MAX_NO_TOOL_RETRIES + 1
                                ));
                            }
                            attempt += 1;
                            continue;
                        }

                        interaction.print_error(&format!(
                            "Step '{}' requires a tool call but none was made after retries. Last response was: '{}'",
                            step.description,
                            outcome.response.chars().take(140).collect::<String>()
                        ));
                        plan.mark_failed(step.id, "No tool call made when required".to_string());
                        interaction.render_plan(&plan);
                        return Err(anyhow::anyhow!(
                            "Execution failed: step required tool call but agent only responded with text"
                        ));
                    }

                    if is_debug && is_explanatory_step(&step.description) {
                        interaction.print_debug("\nResponse:");
                        interaction.print_response(&outcome.response);
                    }
                    plan.mark_done(step.id);
                    interaction.render_plan(&plan);
                    break;
                }
                Err(e) => {
                    plan.mark_failed(step.id, e.to_string());
                    interaction.render_plan(&plan);
                    interaction.print_error(&e.to_string());
                    return Err(e);
                }
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
    let step_lower = step_lower.trim();

    // Skip steps that are purely informational
    if step_lower.starts_with("answer")
        || step_lower.starts_with("summarize")
        || step_lower.starts_with("summary")
        || step_lower.starts_with("report")
        || step_lower.starts_with("explain")
        || step_lower.starts_with("describe")
        || step_lower.starts_with("present")
    {
        return false;
    }

    // Explicit tool invocation intent should always require tool calls.
    if step_lower.contains("use ") && step_lower.contains("tool") {
        return true;
    }

    let tool_verbs = [
        "check", "review", "lint", "analyze", "search", "find", "list", "read", "write", "create",
        "modify", "replace", "delete", "run", "execute", "suggest", "show", "display", "get",
    ];
    tool_verbs.iter().any(|verb| step_lower.starts_with(verb))
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

#[cfg(test)]
mod tests {
    use super::requires_tool_call;

    #[test]
    fn summary_step_does_not_require_tool() {
        assert!(!requires_tool_call("Summarize findings for the user"));
        assert!(!requires_tool_call("Report the final results"));
    }

    #[test]
    fn review_step_requires_tool() {
        assert!(requires_tool_call(
            "Review ferrous/src for implementation issues"
        ));
    }
}
