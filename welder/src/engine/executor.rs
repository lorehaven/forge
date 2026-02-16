use adk_core::Llm;
use anyhow::Result;
use std::fmt::{Debug, Formatter};
use std::{collections::HashMap, future::Future, pin::Pin, sync::Arc};

use crate::model::caller::call_model;

#[derive(Clone)]
pub struct AgentNode {
    pub instruction: String,
    pub model: Arc<dyn Llm>,
    pub children: Vec<String>,
}

impl Debug for AgentNode {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "AgentNode {{ instruction: {}, model name: {} }}",
            self.instruction,
            self.model.name()
        )
    }
}

const RESET: &str = "\x1b[0m";
const CYAN: &str = "\x1b[36m";
const BLUE: &str = "\x1b[34m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const SEP: &str = "────────────────────────────────────────────────────────";

#[must_use]
pub fn execute<'a, S: std::hash::BuildHasher + Sync>(
    name: &'a str,
    input: String,
    agents: &'a HashMap<String, AgentNode, S>,
    depth: usize,
) -> Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>> {
    Box::pin(async move {
        let indent = "  ".repeat(depth);

        println!("\n{CYAN}{SEP}{RESET}\n{BLUE}{indent}Agent ▸ {name}{RESET}\n{CYAN}{SEP}{RESET}");

        let node = agents
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("Agent not found: {name}"))?;

        println!("{YELLOW}{indent}Routing...{RESET}");

        let routing_prompt = format!(
            "{}\n\n\
             If delegation is required, respond ONLY with one of:\n\
             {}\n\
             Otherwise respond ONLY with: SELF\n\n\
             User request:\n{input}",
            node.instruction,
            node.children.join(", "),
        );

        let decision = call_model(node.model.clone(), routing_prompt).await?;
        let decision = decision.trim();

        for child in &node.children {
            if decision == child {
                println!("{YELLOW}{indent}Delegating → {child}{RESET}");
                return execute(child, input, agents, depth + 1).await;
            }
        }

        println!("{YELLOW}{indent}Executing task...{RESET}");

        let execution_prompt = format!("{}\n\nUser request:\n{input}", node.instruction);

        let result = call_model(node.model.clone(), execution_prompt).await?;

        println!("{GREEN}{indent}Completed by {name}{RESET}");

        Ok(result)
    })
}
