use crate::agent::Agent;
use crate::cli::{print_indented, render_plan};
use crate::config::SamplingConfig;
use colored::Colorize;
use std::fmt;

#[derive(Clone, Debug)]
pub enum StepStatus {
    Pending,
    Running,
    Done,
    Failed(String),
}

#[derive(Clone, Debug)]
pub struct PlanStep {
    pub id: usize,
    pub description: String,
    pub status: StepStatus,
}

#[derive(Debug)]
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
                StepStatus::Running => "[→]".bright_yellow(),
                StepStatus::Done => "[✓]".bright_green(),
                StepStatus::Failed(_) => "[✗]".bright_red(),
            };

            writeln!(f, "{} {}. {}", symbol, step.id, step.description)?;
        }
        Ok(())
    }
}

pub async fn execute_plan(
    agent: &mut Agent,
    mut plan: ExecutionPlan,
    sampling: SamplingConfig,
    is_debug: bool,
) -> anyhow::Result<()> {
    for step in plan.steps.clone() {
        plan.mark_running(step.id);
        render_plan(&plan);

        let result = agent
            .stream(&step.description, sampling.clone(), is_debug)
            .await;

        match result {
            Ok(resp) => {
                if is_debug && is_explanatory_step(&step.description) {
                    println!("\n{}", "Response:".bright_black());
                    print_indented(&resp);
                }
                plan.mark_done(step.id);
                render_plan(&plan);
            }
            Err(e) => {
                plan.mark_failed(step.id, e.to_string());
                render_plan(&plan);
                print_indented(&e.to_string());
                return Err(e);
            }
        }

        render_plan(&plan);
    }

    Ok(())
}

fn is_explanatory_step(step: &str) -> bool {
    let s = step.to_lowercase();
    s.starts_with("modify")
        || s.starts_with("replace")
        || s.starts_with("fix")
        || s.starts_with("add")
        || s.starts_with("remove")
}
