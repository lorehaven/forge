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

        // Prepend execution instruction to ensure model uses tools
        let execution_prompt = format!(
            "EXECUTE THIS STEP NOW using the appropriate tool calls. DO NOT just describe what you will do - actually call the tools: {}",
            step.description
        );

        let result = agent
            .stream(&execution_prompt, sampling.clone(), is_debug, interaction)
            .await;

        match result {
            Ok(resp) => {
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
